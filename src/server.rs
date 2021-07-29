use crate::endpoints::Endpoints;
use crate::service::RequestType;
use uuid::Uuid;
use crate::websocket::{WebSocketMessage, ConnectionContext, WebSocketSession};
use crate::{SessionMessage, websocket, service, MetricsMetadata, Request, CloseFrame, CloseCode};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use std::collections::HashMap;
use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::TokioAdapter;
use futures::StreamExt;

use tokio::sync::oneshot::{channel, Sender};
use std::sync::Arc;
use crate::{Body, Response};
use tokio::net::TcpStream;
use std::net::SocketAddr;
use std::future::Future;

#[derive(Debug)]
pub(crate) enum Command {
    WsSession(SessionMessage),
    Metrics(Sender<MetricsMetadata>)
}

#[derive(Clone, Debug)]
pub struct AppContext {
    tx: UnboundedSender<Command>,
}

impl AppContext {
    pub(crate) fn new(tx: UnboundedSender<Command>) -> Self {
        Self { tx }
    }

    pub async fn session(&self, id: Uuid) -> Option<WebSocketSession> {
        let (tx, rx) = channel();
        let _ = self.tx.send(Command::WsSession(SessionMessage::Get(id, Some(tx))));
        match rx.await {
            Ok(data) => data,
            Err(_) => None
        }
    }

    pub async fn metrics(&self) -> Option<MetricsMetadata> {
        let (tx, rx) = channel();
        let _ = self.tx.send(Command::Metrics(tx));
        match rx.await {
            Ok(data) => Some(data),
            Err(_) => None
        }
    }
}

#[derive(Clone, Debug)]
pub struct Server {
    pub(crate) endpoints: Endpoints,
    pub(crate) service_tx: tokio::sync::mpsc::UnboundedSender<RequestType>,
    socket_tx: tokio::sync::mpsc::UnboundedSender<(Uuid, WebSocketMessage)>,
    sessions_tx: tokio::sync::mpsc::UnboundedSender<SessionMessage>,
    command_tx: UnboundedSender<Command>,
}

impl Server {
    pub(crate) fn new(endpoints: Endpoints) -> Self {
        let (sessions_tx, sessions_rx) = unbounded_channel();
        tokio::task::spawn(register_sessions_handle_task(sessions_rx));

        let (socket_tx, mut socket_rx) = unbounded_channel::<(Uuid, WebSocketMessage)>();
        let (service_tx, service_rx) = unbounded_channel::<RequestType>();
        let (command_tx, mut command_rx) = unbounded_channel::<Command>();

        let ctx = AppContext::new(command_tx.clone());
        endpoints.init_handlers(ctx);

        let server = Self {
            endpoints,
            service_tx,
            socket_tx,
            sessions_tx,
            command_tx,
        };

        let server2 = server.clone();
        tokio::task::spawn(async move {
            while let Some(cmd) = command_rx.recv().await {
                match cmd {
                    Command::WsSession(sess_msg) => {
                        let _ = server2.sessions_tx.send(sess_msg);
                    }
                    Command::Metrics(channel) => {
                        let _ = server2.sessions_tx.send(SessionMessage::Metrics(channel));
                    }
                }
            }
        });

        let server2 = server.clone();
        tokio::task::spawn(async move {
            while let Some((id, msg)) = socket_rx.recv().await {
                server2.recv(id, msg).await;
            }
        });

        let server2 = server.clone();
        tokio::task::spawn(register_service_handle_task(server2, service_rx));

        server
    }

    #[tracing::instrument(level = "trace", skip(self, socket, ctx))]
    async fn handle_new_session(
        &self,
        socket: WebSocketStream<TokioAdapter<TcpStream>>,
        ctx: ConnectionContext,
    ) {
        let (ws_session_tx, ws_session_rx) = socket.split();
        let (tx, rx) = unbounded_channel::<WebSocketMessage>();
        let session = WebSocketSession::new(ctx, tx);
        let session_id = session.id();
        let path = session.context().path();

        // async task to receive messages from the web socket connection
        websocket::register_recv_ws_message_handling(
            self.socket_tx.clone(),
            ws_session_rx,
            session_id,
        )
            .await;

        // async task to send messages to the web socket connection
        websocket::register_send_to_ws_message_handling(ws_session_tx, rx).await;

        tracing::trace!("adding session: {:?}, for path: {}", session_id, path);
        if self.sessions_tx.send(SessionMessage::Add(session)).is_ok() {
            self.endpoints.on_open(session_id, path.as_str()).await;
        }
    }

    #[tracing::instrument(level = "trace", skip(self, request))]
    async fn handle_web_request(
        &self,
        request: Request<Body>,
    ) -> Arc<hyper::Result<Response<Body>>> {
        self.endpoints.handle_web_request(request).await
    }

    pub async fn bind(self, addrs: SocketAddr) -> hyper::Result<()> {
        service::HttpService::new(self).serve(addrs).await.await
    }

    /// Removed a web socket session from the server
    #[tracing::instrument(level = "trace", skip(self, session_id))]
    async fn remove_session(&self, session_id: Uuid) {
        let _ = self
            .sessions_tx
            .send(SessionMessage::Remove(session_id, None));
    }

    /// Receive message from the web socket connection
    #[tracing::instrument(level = "trace", skip(self, session_id, message))]
    pub async fn recv(&self, session_id: Uuid, message: WebSocketMessage) {
        let (tx, rx) = channel();
        if self
            .sessions_tx
            .send(SessionMessage::Get(session_id, Some(tx)))
            .is_err()
        {
            tracing::error!("Unable to request WebSocketSession from task");
            return;
        }

        let session = match rx.await {
            Ok(s) => {
                if let Some(s) = s {
                    s
                } else {
                    return;
                }
            }
            Err(_) => return,
        };

        let path = session.context().path();
        let is_close = message.is_close();
        self.endpoints.on_message(session_id, path.as_str(), message).await;
        if is_close {
            self.remove_session(session_id).await;
        }
    }
}

fn register_sessions_handle_task(mut rx: UnboundedReceiver<SessionMessage>) -> impl Future<Output=()> {
    async move {
        let mut sessions = HashMap::new();
        while let Some(data) = rx.recv().await {
            match data {
                SessionMessage::Add(ws) => {
                    let _ = sessions.insert(ws.id(), ws);
                    tracing::trace!("total sessions: {}", sessions.len());
                }
                SessionMessage::Get(id, reply) => {
                    if let Some(s) = sessions.get(&id) {
                        if let Some(channel) = reply {
                            let _ = channel.send(Some(s.clone()));
                        }
                    };
                }
                SessionMessage::Remove(id, reply) => {
                    if let Some(s) = sessions.remove(&id) {
                        if let Some(channel) = reply {
                            let _ = channel.send(Some(s.clone()));
                        }
                        drop(s);
                    }

                    tracing::debug!("Current total sessions: {}", sessions.len());
                }
                SessionMessage::Metrics(reply) => {
                    let total = sessions.len();
                    let mut map = HashMap::new();
                    for value in sessions.iter() {
                        let path = value.1.context().path();
                        if let Some(value) = map.get_mut(&path) {
                            *value += 1usize;
                        } else {
                            map.insert(path, 1usize);
                        }
                    }

                    let _ = reply.send(MetricsMetadata {
                        total_sessions: total,
                        path_counter: map,
                    });
                }
            };
        }
    }
}

fn register_service_handle_task(server: Server, mut service_rx: UnboundedReceiver<RequestType>) -> impl Future<Output=()> {
    async move {
        loop {
            match service_rx.recv().await {
                Some(RequestType::Socket(wrapper)) => {
                    let (ws, ctx) = wrapper.into_parts();
                    server.handle_new_session(ws, ctx).await
                }
                Some(RequestType::Web(wrapper)) => {
                    let (request, channel) = wrapper.into_parts();
                    let result = server.handle_web_request(request).await;
                    if channel.send(result).is_err() {
                        tracing::error!("Send Error!")
                    }
                }
                None => return,
            };
        }
    }
}