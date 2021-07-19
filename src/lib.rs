mod service;

use crate::builder::Builder;
use crate::endpoints::Endpoints;
use crate::websocket::{ConnectionContext, WebSocketMessage, WebSocketSession};

use async_tungstenite::{tokio::TokioAdapter, WebSocketStream};
use futures::StreamExt;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot::{channel, Sender};
use uuid::Uuid;

pub mod builder;
pub mod endpoints;
pub mod webhandler;
pub mod websocket;

use crate::service::RequestType;
pub use async_trait::async_trait;
pub use async_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
pub use hyper::{Body, Request, Response, Result, StatusCode};

#[derive(Clone, Debug)]
pub struct Server {
    endpoints: Endpoints,
    service_tx: tokio::sync::mpsc::UnboundedSender<RequestType>,
    socket_tx: tokio::sync::mpsc::UnboundedSender<(Uuid, WebSocketMessage)>,
    sessions_tx: tokio::sync::mpsc::UnboundedSender<SessionMessage>,
}

/// Internal Message Type to work with the async Task
/// responsible to handle the Web Socket Sessions Collection
#[derive(Debug)]
enum SessionMessage {
    /// Add a new Web Socket Session to the Collection
    Add(WebSocketSession),
    /// Remove the Web Socket Session identified by the Uuid
    Remove(Uuid, Option<Sender<Option<WebSocketSession>>>),
    /// Returns a cloned Web Socket Session identified by the Uuid
    Get(Uuid, Option<Sender<Option<WebSocketSession>>>),
    #[allow(dead_code)]
    Metrics(Sender<MetricsMetadata>),
}

#[derive(Clone, Debug)]
struct MetricsMetadata {
    total_sessions: usize,
    path_counter: HashMap<String, usize>,
}

impl Server {
    pub(crate) fn new(endpoints: Endpoints) -> Self {
        let (sessions_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::task::spawn(async move {
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
                                drop(s);
                            }
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
        });

        let (socket_tx, mut rx) = unbounded_channel::<(Uuid, WebSocketMessage)>();
        let (service_tx, mut service_rx) = unbounded_channel::<RequestType>();
        let server = Self {
            endpoints,
            service_tx,
            socket_tx,
            sessions_tx,
        };
        let server2 = server.clone();
        tokio::task::spawn(async move {
            while let Some((id, msg)) = rx.recv().await {
                server2.recv(id, msg).await;
            }
        });

        let server2 = server.clone();
        tokio::task::spawn(async move {
            loop {
                match service_rx.recv().await {
                    Some(RequestType::Socket(wrapper)) => {
                        let (ws, ctx) = wrapper.into_parts();
                        server2.handle_new_session(ws, ctx).await
                    }
                    Some(RequestType::Web(wrapper)) => {
                        let (request, channel) = wrapper.into_parts();
                        let result = server2.handle_web_request(request).await;
                        if channel.send(result).is_err() {
                            tracing::error!("Send Error!")
                        }
                    }
                    None => return,
                };
            }
        });

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

        // async task to receive messages from the web socket connection
        websocket::register_recv_ws_message_handling(
            self.socket_tx.clone(),
            ws_session_rx,
            session_id,
        )
        .await;

        // async task to send messages to the web socket connection
        websocket::register_send_to_ws_message_handling(ws_session_tx, rx).await;

        self.add_session(session).await;
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

    /// Add a new web socket session to the server after a successful connection upgrade
    #[tracing::instrument(level = "trace", skip(self, session))]
    async fn add_session(&self, session: WebSocketSession) {
        tracing::trace!(
            "adding session: {:?}, for path: {}",
            session.id(),
            session.context().path()
        );
        let session2 = session.clone();
        if self.sessions_tx.send(SessionMessage::Add(session)).is_ok() {
            self.endpoints.on_open(&session2).await;
        }
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

        if message.is_close() {
            tracing::trace!(
                "received message {:?} from session id: {:?}",
                message,
                session_id
            );

            self.endpoints.on_close(&session, message).await;
            self.remove_session(session_id).await;
        } else if message.is_text() || message.is_binary() {
            tracing::trace!(
                "received message {:?} from session id: {:?}",
                message,
                session_id
            );
            self.endpoints.on_message(&session, message).await;
        } else if message.is_ping() || message.is_pong() {
            // tunges is handling this type of messages for us
            tracing::trace!(
                "received message {:?} from session id: {:?}",
                message,
                session_id
            );
        } else {
            unimplemented!("What kind of message is that?!");
        }
    }
}

pub fn builder() -> Builder {
    builder::Builder::new()
}

#[cfg(test)]
mod tests {
    fn test_send_sync<T: Send + Sync>(_server: &T) {}

    #[tokio::test]
    async fn test_server_is_send_and_sync() {
        let endpoints = crate::Endpoints::default();
        let server = crate::Server::new(endpoints);

        test_send_sync(&server);
        assert!(true);
    }
}
