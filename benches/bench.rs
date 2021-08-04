use blunt::rt::mpsc::UnboundedSender;
use blunt::server::AppContext;
use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use uuid::Uuid;

type UserCollection = Arc<RwLock<HashMap<Uuid, UnboundedSender<WebSocketMessage>>>>;
fn user_collection() -> UserCollection {
    Arc::new(RwLock::new(HashMap::new()))
}

#[derive(Debug)]
pub struct EchoServer(AppContext, UserCollection);

#[blunt::async_trait]
impl WebSocketHandler for EchoServer {
    async fn on_open(&mut self, session_id: Uuid) {
        if let Some(ws) = self.0.session(session_id).await {
            self.1.write().await.insert(ws.id(), ws.channel());
        }
    }

    async fn on_message(&mut self, session_id: Uuid, msg: WebSocketMessage) {
        if let Some(ws) = self.1.read().await.get(&session_id) {
            if let Err(e) = ws.send(msg) {
                eprintln!("Unable to send: {:?}", e);
            }
        }
    }

    async fn on_close(&mut self, session_id: Uuid, msg: WebSocketMessage) {
        self.1.write().await.remove(&session_id);
    }
}

fn start_echo_server() -> JoinHandle<()> {
    tokio::spawn(async {
        let _ = ::blunt::builder()
            .for_path_with_ctor("/echo", |ctx| EchoServer(ctx, user_collection()))
            .build()
            .bind("127.0.0.1:9999".parse().expect("Invalid Socket Addr"))
            .await;
    })
}

fn single_echo_benchmark(c: &mut Criterion) {
    use tungstenite::{connect, Message};

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let _server_handle = start_echo_server();
        let (mut socket, _response) = connect("ws://localhost:9999/echo").expect("Can't connect");

        let mut group = c.benchmark_group("single echo server");
        group.throughput(Throughput::Elements(100));
        group.bench_function("100 - Send only", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for _n in 0..iters {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("single echo server");
        group.throughput(Throughput::Elements(100));
        group.bench_function("100 - Send and receive", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for _n in 0..iters {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                    let _ = socket.read_message().expect("Error reading message");
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("single echo server");
        group.throughput(Throughput::Elements(1000));
        group.bench_function("1000 - Send only", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for _n in 0..iters {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("single echo server");
        group.throughput(Throughput::Elements(1000));
        group.bench_function("1000 - Send and receive", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for _n in 0..iters {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                    let _ = socket.read_message().expect("Error reading message");
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("single echo server");
        group.throughput(Throughput::Elements(10000));
        group.bench_function("10000 - Send only", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for _n in 0..iters {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("single echo server");
        group.throughput(Throughput::Elements(10000));
        group.bench_function("10000 - Send and receive", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for _n in 0..iters {
                    socket.write_message(black_box(ws_message.clone())).unwrap();
                    let _ = socket.read_message().expect("Error reading message");
                }

                start.elapsed()
            })
        });

        group.finish();
    });
}

fn start_multi_echo_server() -> JoinHandle<()> {
    tokio::spawn(async {
        let _ = ::blunt::builder()
            .for_path_with_ctor("/echo", |ctx| EchoServer(ctx, user_collection()))
            .for_path_with_ctor("/echo2", |ctx| EchoServer(ctx, user_collection()))
            .build()
            .bind("127.0.0.1:9999".parse().expect("Invalid Socket Addr"))
            .await;
    })
}

fn multi_echo_benchmark(c: &mut Criterion) {
    use tungstenite::{connect, Message};

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let _multi_server_handle = start_multi_echo_server();
        let (mut socket, _response) = connect("ws://localhost:9999/echo").expect("Can't connect");
        let (mut socket2, _response) = connect("ws://localhost:9999/echo2").expect("Can't connect");

        let mut group = c.benchmark_group("multi echo server");
        group.throughput(Throughput::Elements(100));
        group.bench_function("100 - Send only", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for n in 0..iters {
                    if n % 2 == 0 {
                        socket.write_message(black_box(ws_message.clone())).unwrap();
                    } else {
                        socket2
                            .write_message(black_box(ws_message.clone()))
                            .unwrap();
                    }
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("multi echo server");
        group.throughput(Throughput::Elements(100));
        group.bench_function("100 - Send and receive", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for n in 0..iters {
                    if n % 2 == 0 {
                        socket.write_message(black_box(ws_message.clone())).unwrap();
                        let _ = socket.read_message().expect("Error reading message");
                    } else {
                        socket2
                            .write_message(black_box(ws_message.clone()))
                            .unwrap();
                        let _ = socket2.read_message().expect("Error reading message");
                    }
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("multi echo server");
        group.measurement_time(Duration::from_secs(15));
        group.throughput(Throughput::Elements(1000));
        group.bench_function("1000 - Send only", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for n in 0..iters {
                    if n % 2 == 0 {
                        socket.write_message(black_box(ws_message.clone())).unwrap();
                    } else {
                        socket2
                            .write_message(black_box(ws_message.clone()))
                            .unwrap();
                    }
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("multi echo server");
        group.measurement_time(Duration::from_secs(15));
        group.throughput(Throughput::Elements(1000));
        group.bench_function("1000 - Send and receive", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for n in 0..iters {
                    if n % 2 == 0 {
                        socket.write_message(black_box(ws_message.clone())).unwrap();
                        let _ = socket.read_message().expect("Error reading message");
                    } else {
                        socket2
                            .write_message(black_box(ws_message.clone()))
                            .unwrap();
                        let _ = socket2.read_message().expect("Error reading message");
                    }
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("multi echo server");
        group.measurement_time(Duration::from_secs(100));
        group.throughput(Throughput::Elements(10000));
        group.bench_function("10000 - Send only", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for n in 0..iters {
                    if n % 2 == 0 {
                        socket.write_message(black_box(ws_message.clone())).unwrap();
                    } else {
                        socket2
                            .write_message(black_box(ws_message.clone()))
                            .unwrap();
                    }
                }

                start.elapsed()
            })
        });

        group.finish();

        let mut group = c.benchmark_group("multi echo server");
        group.measurement_time(Duration::from_secs(100));
        group.throughput(Throughput::Elements(10000));
        group.bench_function("10000 - Send and receive", |b| {
            b.iter_custom(|iters| {
                let ws_message = Message::Text(String::from("Hello World!"));
                let start = Instant::now();
                for n in 0..iters {
                    if n % 2 == 0 {
                        socket.write_message(black_box(ws_message.clone())).unwrap();
                        let _ = socket.read_message().expect("Error reading message");
                    } else {
                        socket2
                            .write_message(black_box(ws_message.clone()))
                            .unwrap();
                        let _ = socket2.read_message().expect("Error reading message");
                    }
                }

                start.elapsed()
            })
        });

        group.finish();
    });
}

criterion_group!(benches, single_echo_benchmark, multi_echo_benchmark);
criterion_main!(benches);
