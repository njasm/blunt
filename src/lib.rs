mod service;

use crate::builder::Builder;
use crate::endpoints::Endpoints;
use crate::websocket::{ConnectionContext, WebSocketMessage, WebSocketSession};

use std::collections::HashMap;

use async_tungstenite::{tokio::TokioAdapter, WebSocketStream};

use futures::StreamExt;
use std::borrow::Borrow;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};

use uuid::Uuid;

pub mod builder;
pub mod endpoints;
pub mod webhandler;
pub mod websocket;
pub use async_trait::async_trait;
pub use async_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
pub use hyper::{Body, Request, Response, Result, StatusCode};

/// Our WebSocket Session Collection
pub type WebSocketSessions = Arc<RwLock<HashMap<Uuid, WebSocketSession>>>;

#[derive(Clone, Debug)]
pub struct Server {
    endpoints: Endpoints,
    pub sessions: WebSocketSessions,
}

impl Server {
    pub(crate) fn new(endpoints: Endpoints) -> Self {
        Self {
            endpoints,
            sessions: WebSocketSessions::default(),
        }
    }

    #[tracing::instrument(level = "trace", skip(self, socket, ctx))]
    async fn handle_new_session(
        &mut self,
        socket: WebSocketStream<TokioAdapter<TcpStream>>,
        ctx: ConnectionContext,
    ) {
        let (ws_session_tx, ws_session_rx) = socket.split();
        let (tx, rx) = mpsc::unbounded_channel::<WebSocketMessage>();

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
        &mut self,
        request: Request<Body>,
    ) -> Arc<hyper::Result<Response<Body>>> {
        self.endpoints.handle_web_request(request).await
    }

    pub async fn bind(self, addrs: SocketAddr) -> hyper::Result<()> {
        service::HttpService::new(self).serve(addrs).await.await
    }

    /// Add a new web socket session to the server after a successful connection upgrade
    #[tracing::instrument(level = "trace", skip(self, session))]
    async fn add_session(&mut self, session: WebSocketSession) {
        tracing::trace!(
            "adding session: {:?}, for path: {}",
            session.id(),
            session.context().path()
        );
        let session2 = session.clone();
        let len: usize = {
            let mut lock = self.sessions.write().await;
            lock.insert(session.id(), session);
            lock.len()
        };

        tracing::trace!("total sessions: {}", len);
        self.endpoints.on_open(&session2).await;
    }

    /// Removed a web socket session from the server
    #[tracing::instrument(level = "trace", skip(self))]
    async fn remove_session(&self, session_id: Uuid) {
        let s = { self.sessions.write().await.remove(session_id.borrow()) };
        drop(s);

        let len = { self.sessions.read().await.len() };
        tracing::debug!("Current total active sessions: {}", len);
    }

    /// Receive message from the web socket connection
    #[tracing::instrument(level = "trace", skip(self, message))]
    pub async fn recv(&mut self, session_id: Uuid, message: WebSocketMessage) {
        let session = {
            let lock = self.sessions.read().await;
            match lock.get(session_id.borrow()) {
                Some(v) => v.clone(),
                None => return,
            }
        };

        if message.is_close() {
            tracing::trace!(
                "received message {:?} from session id: {:?}",
                message,
                session_id
            );

            self.endpoints.on_close(&session, message).await;
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
            tracing::error!("What kind of message is that?!");
            unimplemented!("What kind of message is that?!");
        }
    }
}

pub fn builder() -> Builder {
    builder::Builder::new()
}


#[cfg(test)]
mod tests {
    fn test_send_sync<T: Send + Sync>(server: T) {}

    #[tokio::test]
    async fn test_server_is_send_and_sync() {
        let endpoints = crate::Endpoints::default();
        let server = crate::Server::new(endpoints);

        test_send_sync(server);
        assert!(true);
    }
}