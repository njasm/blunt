use blunt::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tungstenite::{connect, Message};

#[derive(Debug, Default)]
pub struct EchoServer;

#[blunt::async_trait]
impl WebSocketHandler for EchoServer {
    async fn on_open(&mut self, _ws: &WebSocketSession) {}
    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage) {
        let _ = ws.send(msg);
    }

    async fn on_close(&mut self, _ws: &WebSocketSession, _msg: WebSocketMessage) {}
}

fn start_echo_server() -> JoinHandle<()> {
    tokio::spawn(async {
        let handler = EchoServer::default();
        let _ = ::blunt::builder()
            .for_path("/echo", handler)
            .build()
            .bind("127.0.0.1:9999".parse().expect("Invalid Socket Addr"))
            .await;
    })
}

fn single_server_send_only_iter_custom() -> impl FnMut(u64) -> Duration {
    |iters| {
        let (mut socket, _response) = connect("ws://localhost:9999/echo")
            .expect("Can't connect");
        let ws_message = Message::Text(String::from("Hello World!"));
        let start = Instant::now();
        for _n in 0..iters {
            if let Err(e) = socket.write_message(black_box(ws_message.clone())) {
                eprintln!("error: {:?}", e);
            }
        }

        start.elapsed()
    }
}

fn single_server_send_recv_iter_custom() -> impl FnMut(u64) -> Duration {
    |iters| {
        let ws_message = Message::Text(String::from("Hello World!"));
        let (mut socket, _response) = connect("ws://localhost:9999/echo")
            .expect("Can't connect");

        let start = Instant::now();
        for _n in 0..iters {
            socket.write_message(black_box(ws_message.clone())).unwrap();
            let _ = socket.read_message().expect("Error reading message");
        }

        start.elapsed()
    }
}


enum ServerType {
    SingleServer,
    MultiServer,
}

fn start_tokio_rt(typ: ServerType) -> tokio::sync::mpsc::UnboundedSender<bool> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<bool>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _server_handle = match typ {
                ServerType::SingleServer => start_echo_server(),
                ServerType::MultiServer => start_multi_echo_server()
            };

            let _ = rx.recv().await;
        })
    });

    std::thread::sleep(Duration::from_secs(2));
    tx
}

#[allow(dead_code)]
fn test_async(c: &mut Criterion) {
    let _rt_guard = start_tokio_rt(ServerType::SingleServer);

    let mut group = c.benchmark_group("single echo server");
    group.throughput(Throughput::Elements(100));
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(100);
    group.bench_function("100 - Send only", |b| {
        b.iter_custom(single_server_send_only_iter_custom())
    });

    group.finish();
}

fn single_echo_benchmark(c: &mut Criterion) {
    let _rt_guard = start_tokio_rt(ServerType::SingleServer);

    let mut group = c.benchmark_group("single echo server");
    group.throughput(Throughput::Elements(100));
    group.measurement_time(Duration::from_secs(15));
    group.bench_function("100 - Send only", |b| {
        b.iter_custom(single_server_send_only_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("single echo server");
    group.throughput(Throughput::Elements(100));
    group.bench_function("100 - Send and receive", |b| {
        b.iter_custom(single_server_send_recv_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("single echo server");
    group.throughput(Throughput::Elements(1000));
    group.bench_function("1000 - Send only", |b| {
        b.iter_custom(single_server_send_only_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("single echo server");
    group.throughput(Throughput::Elements(1000));
    group.bench_function("1000 - Send and receive", |b| {
        b.iter_custom(single_server_send_recv_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("single echo server");
    group.throughput(Throughput::Elements(10000));
    group.bench_function("10000 - Send only", |b| {
        b.iter_custom(single_server_send_only_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("single echo server");
    group.throughput(Throughput::Elements(10000));
    group.bench_function("10000 - Send and receive", |b| {
        b.iter_custom(single_server_send_recv_iter_custom())
    });

    group.finish();
}

fn start_multi_echo_server() -> JoinHandle<()> {
    tokio::spawn(async {
        let handler = EchoServer::default();
        let handler2 = EchoServer::default();
        let _ = ::blunt::builder()
            .for_path("/echo", handler)
            .for_path("/echo2", handler2)
            .build()
            .bind("127.0.0.1:9999".parse().expect("Invalid Socket Addr"))
            .await;
    })
}

fn multi_server_send_only_iter_custom() -> impl FnMut(u64) -> Duration {
    |iters| {
        let (mut socket, _response) = connect("ws://localhost:9999/echo").expect("Can't connect");
        let (mut socket2, _response) = connect("ws://localhost:9999/echo2").expect("Can't connect");
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
    }
}

fn multi_server_send_recv_iter_custom() -> impl FnMut(u64) -> Duration {
    |iters| {
        let (mut socket, _response) = connect("ws://localhost:9999/echo").expect("Can't connect");
        let (mut socket2, _response) = connect("ws://localhost:9999/echo2").expect("Can't connect");
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
    }
}

fn multi_echo_benchmark(c: &mut Criterion) {
    let _rt_guard = start_tokio_rt(ServerType::MultiServer);

    let mut group = c.benchmark_group("multi echo server");
    group.throughput(Throughput::Elements(100));
    group.bench_function("100 - Send only", |b| {
        b.iter_custom(multi_server_send_only_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("multi echo server");
    group.throughput(Throughput::Elements(100));
    group.bench_function("100 - Send and receive", |b| {
        b.iter_custom(multi_server_send_recv_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("multi echo server");
    group.measurement_time(Duration::from_secs(15));
    group.throughput(Throughput::Elements(1000));
    group.bench_function("1000 - Send only", |b| {
        b.iter_custom(multi_server_send_only_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("multi echo server");
    group.measurement_time(Duration::from_secs(15));
    group.throughput(Throughput::Elements(1000));
    group.bench_function("1000 - Send and receive", |b| {
        b.iter_custom(multi_server_send_recv_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("multi echo server");
    group.measurement_time(Duration::from_secs(100));
    group.throughput(Throughput::Elements(10000));
    group.bench_function("10000 - Send only", |b| {
        b.iter_custom(multi_server_send_only_iter_custom())
    });

    group.finish();

    let mut group = c.benchmark_group("multi echo server");
    group.measurement_time(Duration::from_secs(100));
    group.throughput(Throughput::Elements(10000));
    group.bench_function("10000 - Send and receive", |b| {
        b.iter_custom(multi_server_send_recv_iter_custom())
    });

    group.finish();
}

criterion_group!(benches, single_echo_benchmark, multi_echo_benchmark);
//criterion_group!(benches, single_echo_benchmark);
//criterion_group!(benches, test_async);
criterion_main!(benches);
