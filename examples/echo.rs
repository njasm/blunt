use async_tungstenite::tungstenite::http::Request;
use blunt::server::AppContext;
use blunt::webhandler::WebHandler;
use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use hyper::header::CONTENT_TYPE;
use hyper::http::HeaderValue;
use hyper::{Body, Response};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use uuid::Uuid;

#[tokio::main]
async fn main() -> hyper::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,blunt=trace");
    }
    // just for a nice compact tracing messages
    FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_thread_ids(true)
        .with_target(true)
        .with_ansi(true)
        .compact()
        .init();

    // just what's actually needed
    let web = HelloWorldWeb::default();
    ::blunt::builder()
        .for_path_with_ctor("/echo", |app| EchoServer { app })
        .for_web_path("/world", web)
        .build()
        .bind("127.0.0.1:3000".parse().expect("Invalid Socket Addr"))
        .await?;

    Ok(())
}

#[derive(Debug)]
pub struct EchoServer {
    app: AppContext,
}

#[blunt::async_trait]
impl WebSocketHandler for EchoServer {
    async fn on_open(&mut self, session_id: Uuid) {
        info!("new connection open with id: {}", session_id);
        self.app
            .session(session_id)
            .await
            .and_then(|s|
                s.send(WebSocketMessage::Text(String::from("Welcome to Echo server!")))
                    .ok()
            );
    }

    async fn on_message(&mut self, session_id: Uuid, msg: WebSocketMessage) {
        info!(
            "echo back for session id {}, with message: {}",
            session_id, msg
        );

        self.app
            .session(session_id)
            .await
            .and_then(|s| s.send(msg).ok());
    }

    async fn on_close(&mut self, session_id: Uuid, _msg: WebSocketMessage) {
        info!("connection closed for session id {}", session_id);
    }
}

#[derive(Debug, Default)]
pub struct HelloWorldWeb;

#[blunt::async_trait]
impl WebHandler for HelloWorldWeb {
    async fn handle(&mut self, request: Request<Body>) -> Arc<hyper::Result<Response<Body>>> {
        let message = format!("Hello World from path: {}", request.uri().path());
        Arc::new(Ok(Response::builder()
            .header(
                CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )
            .body(Body::from(message))
            .unwrap()))
    }
}
