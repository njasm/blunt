use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

#[derive(Debug, Default)]
pub struct EchoServer;

#[blunt::async_trait]
impl WebSocketHandler for EchoServer {
    async fn on_open(&mut self, _ws: &WebSocketSession) {}
    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage) {
        ws.send(msg).expect("Unable to send message");
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
        let handler = EchoServer::default();
        let handler2 = EchoServer::default();
        let _ = ::blunt::builder()
            .for_path("/echo", handler)
            .for_path("/echo2", handler2)
            .build()
            .bind("127.0.0.1:9999")
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
