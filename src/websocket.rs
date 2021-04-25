use crate::Server;
use async_tungstenite::tungstenite::http::HeaderMap;
use core::fmt::Debug;
use futures::{
    stream::FuturesUnordered,
    task::{Context, Poll},
    Future, StreamExt, TryStreamExt,
};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::mpsc::Sender;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use uuid::Uuid;

/// Async task to receive messages from the web socket connection
pub(crate) async fn register_recv_ws_message_handling(
    mut server: Server,
    mut rx: UnboundedReceiver<(Uuid, WebSocketMessage)>,
) {
    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            server.recv(data.0, data.1).await;
        }
    });
}

/// Our Websocket Message
pub type WebSocketMessage = async_tungstenite::tungstenite::protocol::Message;

use async_trait::async_trait;

#[async_trait]
pub trait WebSocketHandler: Sync + Send + Debug {
    async fn on_open(&mut self, ws: &WebSocketSession);
    async fn on_transport_error(&mut self, ws: WebSocketSession);
    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage);
    async fn on_close(&mut self, ws: &WebSocketSession, msg: WebSocketMessage);
}

/// Our websocket wrapper
#[derive(Clone, Debug)]
pub struct WebSocketSession {
    // session id
    id: Uuid,
    // this connection context
    ctx: ConnectionContext,
    // buffer, send to socket messages
    tx: UnboundedSender<WebSocketMessage>,
}

impl WebSocketSession {
    pub fn new(ctx: ConnectionContext, tx: UnboundedSender<WebSocketMessage>) -> Self {
        Self {
            id: Uuid::new_v4(),
            ctx,
            tx,
        }
    }

    /// Returns web socket session id
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Send message to the web socket connection
    pub fn send(&self, message: WebSocketMessage) {
        self.tx.send(message).unwrap();
    }

    /// Returns the http request context for this web socket connection
    pub fn context(&self) -> &ConnectionContext {
        &self.ctx
    }
}

/// Web socket connection context
#[derive(Clone, Debug)]
pub struct ConnectionContext {
    addr: Option<SocketAddr>,
    headers: HeaderMap,
    query: Option<String>,
    path: String,
}

impl ConnectionContext {
    pub fn new(addr: Option<SocketAddr>, headers: HeaderMap, query: String, path: String) -> Self {
        Self {
            addr,
            headers,
            query: if query.is_empty() { None } else { Some(query) },
            path,
        }
    }

    pub fn addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    pub fn headers(&self) -> HeaderMap {
        self.headers.clone()
    }

    pub fn path(&self) -> String {
        self.path.clone()
    }

    pub fn query(&self) -> Option<String> {
        self.query.clone()
    }
}
