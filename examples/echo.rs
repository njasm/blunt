use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};

#[tokio::main]
async fn main() -> std::io::Result<()> {

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
        println!("new connection open with id: {}", ws.id());
    }

    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage) {
        println!("echo back for session id {}, with message: {}",
            ws.id(),
            msg
        );
        ws.send(msg);
    }

    async fn on_close(&mut self, ws: &WebSocketSession, _msg: WebSocketMessage) {
        println!("connection closed for session id {}", ws.id());
    }
}
