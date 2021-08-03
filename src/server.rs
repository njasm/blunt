use crate::endpoints::{Endpoints, HandleWeb};
use crate::service::{RequestType, WebConnWrapper};
use crate::websocket::{ConnectionContext, WebSocketMessage, WebSocketSession};
use crate::{service, spawn, websocket, MetricsMetadata, Request, SessionMessage};
use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::WebSocketStream;
use futures::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use uuid::Uuid;

use crate::{Body, Response};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::oneshot::{channel, Sender};

#[derive(Debug)]
pub(crate) enum Command {
    // Generic web socket session's context internal messages
    WsSession(SessionMessage),
    // handle web socket inbound message
    WebSocketMessageReceiving(Uuid, WebSocketMessage),
    // handle tower service requests (http requests)
    Handle(RequestType),
    Metrics(Sender<MetricsMetadata>),
}

#[derive(Clone, Debug)]
pub struct AppContext {
    tx: UnboundedSender<Command>,
    path: String,
}

impl AppContext {
    pub(crate) fn new(tx: UnboundedSender<Command>, path: String) -> Self {
        Self { tx, path }
    }

    pub async fn session(&self, id: Uuid) -> Option<WebSocketSession> {
        let (tx, rx) = channel();
        self.tx
            .send(Command::WsSession(SessionMessage::Get(id, Some(tx))))
            .ok();

        rx.await.ok()?
    }

    pub async fn metrics(&self) -> Option<MetricsMetadata> {
        let (tx, rx) = channel();
        let _ = self.tx.send(Command::Metrics(tx));
        match rx.await {
            Ok(data) => Some(data),
            Err(_) => None,
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
    pub(crate) fn new(
        endpoints: Endpoints,
        channels: (UnboundedSender<Command>, UnboundedReceiver<Command>),
    ) -> Self {
        let (sessions_tx, sessions_rx) = unbounded_channel();
        spawn(register_sessions_handle_task(sessions_rx));

        let (socket_tx, mut socket_rx) = unbounded_channel::<(Uuid, WebSocketMessage)>();
        let (service_tx, service_rx) = unbounded_channel::<RequestType>();
        let (command_tx, mut command_rx) = channels;

        let server = Self {
            endpoints,
            service_tx,
            socket_tx,
            sessions_tx,
            command_tx,
        };

        let server2 = server.clone();
        spawn(async move {
            while let Some(cmd) = command_rx.recv().await {
                match cmd {
                    Command::WsSession(sess_msg) => {
                        server2.sessions_tx.send(sess_msg).ok();
                    }
                    Command::Metrics(channel) => {
                        server2
                            .sessions_tx
                            .send(SessionMessage::Metrics(channel))
                            .ok();
                    }
                    Command::WebSocketMessageReceiving(id, msg) => {
                        server2.recv(id, msg).await;
                    }
                    Command::Handle(request_type) => match request_type {
                        RequestType::Socket(wrapper) => {
                            let (ws, ctx) = wrapper.into_parts();
                            server2.handle_new_session(ws, ctx).await;
                        }
                        RequestType::Web(wrapper) => {
                            let (request, channel) = wrapper.into_parts();
                            let result = server2.handle_web_request(request).await;
                            if channel.send(result).is_err() {
                                tracing::error!("Send Error!")
                            }
                        }
                    },
                }
            }
        });

        let cmd_tx2 = server.command_tx.clone();
        spawn(async move {
            while let Some((id, msg)) = socket_rx.recv().await {
                let _ = cmd_tx2.send(Command::WebSocketMessageReceiving(id, msg));
            }
        });

        let cmd_tx2 = server.command_tx.clone();
        spawn(register_service_handle_task(cmd_tx2, service_rx));

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
            self.endpoints
                .handle(path.as_str(), HandleWeb::SocketOpen(session_id));
        }
    }

    #[tracing::instrument(level = "trace", skip(self, request))]
    async fn handle_web_request(
        &self,
        request: Request<Body>,
    ) -> Arc<hyper::Result<Response<Body>>> {
        let (tx, mut rx) = unbounded_channel::<Arc<hyper::Result<Response<Body>>>>();
        let path = request.uri().path().to_string();
        let wrapper = WebConnWrapper::new(request, tx);
        self.endpoints.handle(&path, HandleWeb::Request(wrapper));

        match rx.recv().await {
            Some(t) => t,
            None => unreachable!("Ups, we should not get here. tx dropped already"),
        }
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
            .send(SessionMessage::GetPath(session_id, tx))
            .is_err()
        {
            tracing::error!("Unable to request WebSocketSession from task");
            return;
        }

        let path = match rx.await {
            Ok(Some(path)) => path,
            _ => return,
        };

        let is_close = message.is_close();
        self.endpoints
            .handle(path.as_str(), HandleWeb::SocketMessage(session_id, message));

        if is_close {
            self.remove_session(session_id).await;
        }
    }
}

async fn register_sessions_handle_task(mut rx: UnboundedReceiver<SessionMessage>) {
    let mut sessions = HashMap::new();
    while let Some(data) = rx.recv().await {
        match data {
            SessionMessage::Add(ws) => {
                sessions.insert(ws.id(), ws);
                tracing::debug!("After insert, total sessions: {}", sessions.len());
            }
            SessionMessage::GetPath(id, reply) => {
                let path = sessions.get(&id).map(|p| p.context().path());

                reply.send(path).ok();
            }
            SessionMessage::Get(id, reply) => {
                if let Some(s) = sessions.get(&id) {
                    if let Some(channel) = reply {
                        let _ = channel.send(Some(s.clone()));
                    }
                }
            }
            SessionMessage::Remove(id, reply) => {
                if let Some(s) = sessions.remove(&id) {
                    if let Some(channel) = reply {
                        channel.send(Some(s)).ok();
                    }
                }

                tracing::debug!("After remove, total sessions: {}", sessions.len());
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

async fn register_service_handle_task(
    tx: UnboundedSender<Command>,
    mut service_rx: UnboundedReceiver<RequestType>,
) {
    loop {
        match service_rx.recv().await {
            Some(request_type) => tx.send(Command::Handle(request_type)).ok(),
            None => return,
        };
    }
}
