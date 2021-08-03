use async_tungstenite::tungstenite::http::HeaderMap;
use core::fmt::Debug;

use async_trait::async_trait;
use std::net::SocketAddr;

use crate::spawn;
use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::WebSocketStream;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{error, trace, trace_span, warn};
use tracing_futures::Instrument;
use uuid::Uuid;

/// Async task to receive messages from the web socket connection
pub(crate) async fn register_recv_ws_message_handling(
    server_socket_tx: UnboundedSender<(Uuid, WebSocketMessage)>,
    mut ws_session_rx: SplitStream<WebSocketStream<TokioAdapter<TcpStream>>>,
    session_id: impl Into<Uuid>,
) {
    let session_id = session_id.into();
    spawn(
        async move {
            while let Some(result) = ws_session_rx.next().await {
                match result {
                    Ok(msg) => {
                        server_socket_tx.send((session_id, msg)).ok();
                    }
                    Err(e) => {
                        let error_message = format!("Receiving from websocket: {:?}", e);
                        error!("{}", error_message);

                        let frame = CloseFrame {
                            code: CloseCode::Abnormal,
                            reason: std::borrow::Cow::Owned(error_message),
                        };

                        server_socket_tx
                            .send((session_id, Message::Close(Some(frame))))
                            .ok();
                        return;
                    }
                }
            }
        }
        .instrument(trace_span!("recv_from_ws_task")),
    );
}

/// Async task to send messages to the web socket connection
pub(crate) async fn register_send_to_ws_message_handling(
    mut ws_session_tx: SplitSink<WebSocketStream<TokioAdapter<TcpStream>>, WebSocketMessage>,
    mut rx: UnboundedReceiver<WebSocketMessage>,
) {
    spawn(
        async move {
            while let Some(result) = rx.recv().await {
                trace!("Sending to websocket: {:?}", result);
                if ws_session_tx.send(result).await.is_err() {
                    warn!("Dropping channel server -> 'ws_session_rx'");
                    return;
                }
            }
        }
        .instrument(trace_span!("send_to_ws_task")),
    );
}

/// Our Websocket Message
pub type WebSocketMessage = async_tungstenite::tungstenite::protocol::Message;

pub type CloseFrame<'t> = async_tungstenite::tungstenite::protocol::CloseFrame<'t>;
pub type CloseCode = async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

#[async_trait]
pub trait WebSocketHandler: Sync + Send + Debug {
    async fn on_open(&mut self, session_id: Uuid);
    async fn on_message(&mut self, session_id: Uuid, msg: WebSocketMessage) {
        match msg {
            WebSocketMessage::Text(s) => self.on_message_text(session_id, s).await,
            WebSocketMessage::Binary(b) => self.on_message_binary(session_id, b).await,
            WebSocketMessage::Ping(b) => self.on_message_ping(session_id, b).await,
            WebSocketMessage::Pong(b) => self.on_message_ping(session_id, b).await,
            WebSocketMessage::Close(frame) => self.on_message_close(session_id, frame).await,
        };
    }

    async fn on_message_text(&mut self, _: Uuid, _: String) {}
    async fn on_message_binary(&mut self, _: Uuid, _: Vec<u8>) {}
    async fn on_message_ping(&mut self, _: Uuid, _: Vec<u8>) {}
    async fn on_message_pong(&mut self, _: Uuid, _: Vec<u8>) {}

    async fn on_message_close(&mut self, session_id: Uuid, frame: Option<CloseFrame<'static>>) {
        //fixme: this could should be removed after the deprecation of
        // self::on_close() for WS Close Message processing
        self.on_close(session_id, WebSocketMessage::Close(frame))
            .await;
    }

    async fn on_close(&mut self, session_id: Uuid, msg: WebSocketMessage);
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

    pub(crate) fn from_parts(parts: hyper::http::request::Parts) -> Self {
        let query = match parts.uri.query() {
            Some(s) => s.to_string(),
            _ => String::with_capacity(0),
        };

        ConnectionContext::new(None, parts.headers, query, parts.uri.path().to_owned())
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
