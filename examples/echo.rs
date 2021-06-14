use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use blunt::webhandler::WebHandler;
use std::sync::Arc;
use async_tungstenite::tungstenite::http::Request;
use hyper::{Body, Response};

#[tokio::main]
async fn main() -> hyper::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "blunt=trace");
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
    let handler = EchoServer::default();
    let web = HelloWorldWeb::default();
    ::blunt::builder()
        .for_path("/echo", handler)
        .for_web_path("/world", web)
        .build()
        .bind("127.0.0.1:3000".parse().expect("Invalid Socket Addr"))
        .await?;

    Ok(())
}

#[derive(Debug, Default)]
pub struct EchoServer;

#[blunt::async_trait]
impl WebSocketHandler for EchoServer {
    async fn on_open(&mut self, ws: &WebSocketSession) {
        info!("new connection open with id: {}", ws.id());
    }

    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage) {
        info!(
            "echo back for session id {}, with message: {}",
            ws.id(),
            msg
        );
        ws.send(msg).expect("Unable to send message");
    }

    async fn on_close(&mut self, ws: &WebSocketSession, _msg: WebSocketMessage) {
        info!("connection closed for session id {}", ws.id());
    }
}

#[derive(Debug, Default)]
pub struct HelloWorldWeb;

#[blunt::async_trait]
impl WebHandler for HelloWorldWeb {
    async fn handle(&mut self, request: Arc<Request<Body>>) -> Arc<hyper::Result<Response<Body>>> {
        let message = format!("Hello World from path: {}", request.uri().path());
        Arc::new(Ok(Response::new(Body::from(message))))
    }
}
