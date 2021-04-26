use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "blunt=trace,info");
    FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_thread_ids(true)
        .with_target(true)
        .with_ansi(true)
        .compact()
        .init();

    let handler = EchoServer::default();
    ::blunt::builder()
        .for_path("/echo", handler)
        .build()
        .bind("127.0.0.1:3000")
        .await?;

    Ok(())
}

#[derive(Debug, Default)]
pub struct EchoServer;

#[blunt::async_trait]
impl WebSocketHandler for EchoServer {
    async fn on_open(&mut self, ws: &WebSocketSession) {
        tracing::info!("new connection open with id: {}", ws.id());
    }

    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage) {
        tracing::info!(
            "echo-ing back for session id {}, with message: {}",
            ws.id(),
            msg
        );
        ws.send(msg);
    }

    async fn on_close(&mut self, ws: &WebSocketSession, _msg: WebSocketMessage) {
        tracing::info!("connection closed for session id {}", ws.id());
    }
}
