#![allow(dead_code)]
#![allow(unused_imports)]

use crate::builder::Builder;
use crate::endpoints::Endpoints;
use crate::websocket::{ConnectionContext, WebSocketHandler, WebSocketMessage, WebSocketSession};

use std::collections::HashMap;

use async_tungstenite::{
    tokio::accept_hdr_async_with_config,
    tungstenite::{http::Request, Error},
    WebSocketStream,
};

use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::tungstenite::http::{Response, StatusCode};
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Status;
use async_tungstenite::tungstenite::protocol::CloseFrame;
use futures::future::BoxFuture;
use futures::StreamExt;
use futures::{SinkExt, Stream, TryFutureExt};
use std::borrow::Borrow;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::sync::{mpsc, RwLock};
use tracing_futures::Instrument;
use uuid::Uuid;

pub mod builder;
pub mod endpoints;
pub mod websocket;

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

    async fn handle_new_session(
        &mut self,
        socket: WebSocketStream<TokioAdapter<TcpStream>>,
        ctx: ConnectionContext,
    ) {
        let (mut ws_session_tx, mut ws_session_rx) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<WebSocketMessage>();
        let (server_tx, server_rx) = mpsc::unbounded_channel::<(Uuid, WebSocketMessage)>();

        // async task to receive messages from the web socket connection
        let ws_server2 = self.clone();
        websocket::register_recv_ws_message_handling(ws_server2, server_rx).await;

        // async task to send messages to the web socket connection
        tokio::spawn(async move {
            while let Some(result) = rx.recv().await {
                tracing::trace!("Sending message to websocket connection: {:?}", result);
                ws_session_tx.send(result).await.unwrap();
            }
        });

        let session = WebSocketSession::new(ctx, tx);
        let session_id = session.id();
        self.add_session(session).await;

        // async task to process any incoming messages from the web socket connection
        let server_tx2 = server_tx.clone();
        let f = async move {
            while let Some(result) = ws_session_rx.next().await {
                match result {
                    Ok(msg) => {
                        match server_tx2.send((session_id, msg)) {
                            Ok(_) => {}
                            Err(e) => {
                                tracing::error!("ERROR SENDING TO server_tx2 channel: {}", e)
                            }
                        };
                    }
                    Err(e) => {
                        tracing::error!("ws_session_rx.next() error: {}", e);
                        return;
                    }
                }
            }

            tracing::trace!("we are leaving the gibson - channel dropped");
        };

        tokio::task::spawn(f);
    }

    async fn handle_connection(
        &mut self,
        peer: SocketAddr,
        stream: TcpStream,
    ) -> async_tungstenite::tungstenite::Result<()> {
        type HttpResponse = async_tungstenite::tungstenite::http::Response<()>;

        let (tx, rx) = tokio::sync::oneshot::channel();

        let callback = move |request: &Request<()>, mut response: HttpResponse| {
            let header_options = request.headers().clone();
            let path = request.uri().path().to_owned();
            let query = match request.uri().query() {
                Some(s) => Some(s.to_string()),
                _ => Some("".to_string()),
            };

            if tx.send((header_options, path, query)).is_err() {
                tracing::error!("Unable to upgrade connection");
                let status = response.status_mut();
                *status = StatusCode::from_u16(500).unwrap();
            }

            Ok(response)
        };

        let mut ws_stream = accept_hdr_async_with_config(stream, callback, None)
            .await
            .expect("failed");

        let conn_data = match rx.await {
            Ok(d) => d,
            _ => panic!("Unable to receive connection context for upgrade"),
        };

        if !self.endpoints.contains_path(&conn_data.1).await {
            let err_message = format!(
                "Unable to find Web socket handler for path: {}",
                conn_data.1
            );
            tracing::trace!("{}", err_message);
            let frame = CloseFrame {
                code: CloseCode::Policy,
                reason: Default::default(),
            };

            let _ = ws_stream.close(Some(frame)).await;

            return Err(Error::Http(
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(None)
                    .unwrap(),
            ));
        }

        let ctx = ConnectionContext::new(
            Some(peer),
            conn_data.0,
            conn_data.2.unwrap(),
            conn_data.1.to_string(),
        );
        self.handle_new_session(ws_stream, ctx).await;

        Ok(())
    }

    async fn accept_connection(&mut self, peer: SocketAddr, stream: TcpStream) {
        if let Err(e) = self.handle_connection(peer, stream).await {
            match e {
                Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
                err => tracing::error!("Error processing connection: {}", err),
            }
        }
    }

    pub async fn bind(&mut self, addrs: impl ToSocketAddrs) -> std::io::Result<()> {
        let listener = TcpListener::bind(addrs).await?;

        while let Ok((stream, _sock_addr)) = listener.accept().await {
            match stream.peer_addr() {
                Ok(addr) => {
                    tracing::info!("connected stream's peer address: {}", addr);
                    self.accept_connection(addr, stream).await
                }
                Err(_) => {
                    tracing::warn!("connected streams should have a peer address");
                }
            };
        }

        Ok(())
    }

    /// Add a new web socket session to the server after a successful connection upgrade
    #[tracing::instrument(level = "trace")]
    async fn add_session(&mut self, session: WebSocketSession) {
        tracing::trace!("adding session: {:?}", session.id());
        let session2 = session.clone();
        let len: usize = {
            let mut lock = self.sessions.write().await;
            lock.insert(session.id(), session);
            lock.len()
        };

        tracing::trace!("total sessions: {}", len);
        self.trigger_on_open(&session2).await;
    }

    /// Removed a web socket session from the server
    #[tracing::instrument(level = "trace")]
    async fn remove_session(&self, session_id: Uuid) {
        let s = self.sessions.write().await.remove(session_id.borrow());
        drop(s);

        tracing::debug!(
            "Current total active sessions: {}",
            self.sessions.read().await.len()
        );
    }

    /// Notify Handler that a new web socket session was opened
    #[tracing::instrument(level = "trace")]
    pub async fn trigger_on_open(&mut self, session: &WebSocketSession) {
        self.endpoints.on_open(session).await;
    }

    /// Receive message from the web socket connection
    #[tracing::instrument(level = "trace")]
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
            self.remove_session(session_id).await;

            //self.remove_session(session_id).await;
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
