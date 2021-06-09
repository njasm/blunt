use crate::Server;
use async_tungstenite::tungstenite::http::HeaderMap;
use core::fmt::Debug;

use async_trait::async_trait;
use std::net::SocketAddr;

use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use async_tungstenite::tungstenite::protocol::CloseFrame;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::WebSocketStream;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use uuid::Uuid;

/// Async task to receive messages from the web socket connection
pub(crate) async fn register_recv_ws_message_handling(
    mut server: Server,
    mut ws_session_rx: SplitStream<WebSocketStream<TokioAdapter<TcpStream>>>,
    session_id: impl Into<Uuid>,
) {
    let session_id = session_id.into();
    tokio::spawn(async move {
        while let Some(result) = ws_session_rx.next().await {
            match result {
                Ok(msg) => server.recv(session_id, msg).await,
                Err(e) => {
                    let error_message = format!("Receive from websocket: {:?}", e);
                    tracing::error!("{}", error_message);

                    let frame = CloseFrame {
                        code: CloseCode::Abnormal,
                        reason: std::borrow::Cow::Owned(error_message),
                    };

                    tracing::warn!("Dropping channel 'ws_session_rx' -> server::recv()");
                    server.recv(session_id, Message::Close(Some(frame))).await;
                    return;
                }
            }
        }

        tracing::warn!("we are leaving the gibson - channel dropped");
    });
}

/// Async task to send messages to the web socket connection
pub(crate) async fn register_send_to_ws_message_handling(
    mut ws_session_tx: SplitSink<WebSocketStream<TokioAdapter<TcpStream>>, WebSocketMessage>,
    mut rx: UnboundedReceiver<WebSocketMessage>,
) {
    tokio::spawn(async move {
        while let Some(result) = rx.recv().await {
            tracing::trace!("Sending to websocket: {:?}", result);
            if let Err(e) = ws_session_tx.send(result).await {
                tracing::error!("Sending to websocket: {:?}", e);
                tracing::warn!("Dropping channel server -> 'ws_session_rx'");
                return;
            }
        }
    });
}

/// Our Websocket Message
pub type WebSocketMessage = async_tungstenite::tungstenite::protocol::Message;

#[async_trait]
pub trait WebSocketHandler: Sync + Send + Debug {
    async fn on_open(&mut self, ws: &WebSocketSession);
    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage);
    async fn on_close(&mut self, ws: &WebSocketSession, msg: WebSocketMessage);
}

/// Our websocket wrapper
#[derive(Clone, Debug)]
pub struct WebSocketSession {
    /// session id
    id: Uuid,
    /// this connection context
    ctx: ConnectionContext,
    /// buffer, send to socket messages
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
    pub fn send(&self, message: WebSocketMessage) -> Result<(), SendError<WebSocketMessage>> {
        self.tx.send(message)
    }

    /// Provides a Sender channel to send messages to the web socket connection
    pub fn channel(&self) -> UnboundedSender<WebSocketMessage> {
        self.tx.clone()
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
