use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::task::JoinHandle;

#[derive(Debug, Default)]
pub struct EchoServer;

#[blunt::async_trait]
impl WebSocketHandler for EchoServer {
    async fn on_open(&mut self, _ws: &WebSocketSession) {}
    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage) {
        ws.send(msg);
    }

    async fn on_close(&mut self, _ws: &WebSocketSession, _msg: WebSocketMessage) {}
}

fn start_echo_server() -> JoinHandle<()> {
    tokio::spawn(async {
        let handler = EchoServer::default();
        let _ = ::blunt::builder()
            .for_path("/echo", handler)
            .build()
            .bind("127.0.0.1:9999")
            .await;
    })
}

fn echo_benchmark(c: &mut Criterion) {
    use tungstenite::{connect, Message};

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let _server_handle = start_echo_server();
        let (mut socket, _response) = connect("ws://localhost:9999/echo").expect("Can't connect");

        c.bench_function("echo server 100", |b| {
            b.iter(|| {
                let ws_message = Message::Text(String::from("Hello World!"));
                for _n in 1..=100 {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                    let _ = socket.read_message().expect("Error reading message");
                }
            })
        });

        c.bench_function("echo server 1000", |b| {
            b.iter(|| {
                let ws_message = Message::Text(String::from("Hello World!"));
                for _n in 1..=1000 {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                    let _ = socket.read_message().expect("Error reading message");
                }
            })
        });

        c.bench_function("echo server 10000", |b| {
            b.iter(|| {
                let ws_message = Message::Text(String::from("Hello World!"));
                for _n in 1..=10000 {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                    let _ = socket.read_message().expect("Error reading message");
                }
            })
        });
    });
}

criterion_group!(benches, echo_benchmark);
criterion_main!(benches);
