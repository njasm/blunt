use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use tracing::subscriber::Interest;
use tracing::{span, Event, Level, Metadata, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, FmtSubscriber, Layer};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "blunt=trace,info");

    let curr_path = std::env::current_dir()?;
    // file rolling
    let file_appender = tracing_appender::rolling::hourly(curr_path, "blunt.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let stdout = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_thread_ids(true)
        .with_target(true)
        .with_ansi(true)
        .compact()
        .finish();

    let file_layer = tracing_subscriber::fmt::layer()
        .with_thread_ids(true)
        .with_target(true)
        .with_writer(non_blocking)
        .with_ansi(false)
        .compact();

    let stdout = stdout.with(file_layer);

    tracing::dispatcher::set_global_default(tracing::dispatcher::Dispatch::new(stdout)).unwrap();

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
    #[tracing::instrument(level = "trace")]
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
