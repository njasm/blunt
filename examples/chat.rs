use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;
use uuid::Uuid;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let handler = ChatServer::default();
    ::blunt::builder()
        .for_path("/chat", handler)
        .build()
        .bind("127.0.0.1:3000")
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

    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage) {
        // for the sake of the example, lets just handle text messages
        let final_msg = match msg {
            WebSocketMessage::Text(t) => format!("{}: {}", ws.id(), t),
            _ => format!("{}: {}", ws.id(), ""),
        };

        self.broadcast(ws.id(), WebSocketMessage::Text(final_msg))
            .await;
    }

    async fn on_close(&mut self, ws: &WebSocketSession, _msg: WebSocketMessage) {
        let _ = { self.0.write().await.remove(&ws.id()) };

        let msg = format!("User {} left the chat.", ws.id());
        self.broadcast(ws.id(), WebSocketMessage::Text(msg)).await;
    }
}
