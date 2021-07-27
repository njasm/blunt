use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use uuid::Uuid;

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

    let handler = ChatServer::default();
    ::blunt::builder()
        .for_path("/chat", handler)
        .build()
        .bind("127.0.0.1:3000".parse().expect("Invalid Socket Addr"))
        .await?;

    Ok(())
}

type UserCollection = Arc<RwLock<HashMap<Uuid, UnboundedSender<WebSocketMessage>>>>;

#[derive(Debug, Default)]
pub struct ChatServer(UserCollection);

impl ChatServer {
    async fn broadcast(&mut self, except_id: Uuid, msg: WebSocketMessage) {
        self.0.read().await.iter().for_each(|entry| {
            if entry.0 != &except_id {
                let _ = entry.1.send(msg.clone());
            }
        });
    }
}

#[blunt::async_trait]
impl WebSocketHandler for ChatServer {
    async fn on_open(&mut self, ws: &WebSocketSession) {
        {
            self.0.write().await.insert(ws.id(), ws.channel());
        }

        ws.send(WebSocketMessage::Text(String::from("Welcome!")))
            .expect("Unable to send message");

        let msg = format!("User {} joined the chat.", ws.id());
        self.broadcast(ws.id(), WebSocketMessage::Text(msg)).await;
    }

    async fn on_message_text(&mut self, ws: &WebSocketSession, msg: String) {
        self.broadcast(ws.id(), WebSocketMessage::Text(msg)).await;
    }

    async fn on_close(&mut self, ws: &WebSocketSession, _msg: WebSocketMessage) {
        let (session, len) = {
            let mut guard = self.0.write().await;
            (guard.remove(&ws.id()), guard.len())
        };

        drop(session);

        let msg = format!("User {} left the chat. (current users: {})", ws.id(), len);
        self.broadcast(ws.id(), WebSocketMessage::Text(msg)).await;
    }
}
