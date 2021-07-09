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

pub use async_trait::async_trait;
pub use async_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
pub use hyper::{Body, Request, Response, Result, StatusCode};

#[derive(Clone, Debug)]
pub struct Server {
    endpoints: Endpoints,
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
}

impl Server {
    pub(crate) fn new(endpoints: Endpoints) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
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
                };
            }
        });

        Self {
            endpoints,
            sessions_tx: tx,
        }
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
        let ws_server2 = self.clone();
        websocket::register_recv_ws_message_handling(ws_server2, ws_session_rx, session_id).await;

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
        let _ = self.sessions_tx.send(SessionMessage::Add(session));
        self.endpoints.on_open(&session2).await;
    }

    /// Removed a web socket session from the server
    #[tracing::instrument(level = "trace", skip(self))]
    async fn remove_session(&self, session_id: Uuid) {
        let _ = self
            .sessions_tx
            .send(SessionMessage::Remove(session_id, None));
    }

    /// Receive message from the web socket connection
    #[tracing::instrument(level = "trace", skip(self, message))]
    pub async fn recv(&mut self, session_id: Uuid, message: WebSocketMessage) {
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
            // FIXME: we have &mut self just because this call
            self.endpoints.remove_ws_channel(&session);
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
