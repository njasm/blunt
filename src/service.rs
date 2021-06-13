use crate::websocket::ConnectionContext;
use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::tungstenite::handshake::server::create_response as tungstenite_create_response;
use async_tungstenite::tungstenite::handshake::server::Request as TungsteniteRequest;
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use async_tungstenite::tungstenite::protocol::{CloseFrame, Role};
use async_tungstenite::WebSocketStream;
use hyper::header::UPGRADE;
use hyper::server::conn::{AddrIncoming, AddrStream, Http};
use hyper::service::Service;
use hyper::{Body, Request, Response, Server, StatusCode};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;
use tokio::sync::oneshot::{channel, Sender};

pub(crate) struct Router {
    tx: Option<Sender<Connection>>,
    paths: Vec<&'static str>
}

impl Service<Request<Body>> for Router {
    type Response = Response<Body>;
    type Error = hyper::Error;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // favicon.ico
        if req.uri().path() == "/favicon.ico" {
            let mut res = Response::new(Body::empty());
            *res.status_mut() = StatusCode::NO_CONTENT;
            return Box::pin(async { Ok(res) });
        }

        // check here if we have any of this endpoints registered
        if !self.paths.iter().any(|p| *p == req.uri().path()) {
            tracing::warn!("Not found path: {}", req.uri().path());
            let mut res = Response::new(Body::empty());
            *res.status_mut() = StatusCode::NOT_FOUND;
            return Box::pin(async { Ok(res) });
        }

        // Send a 400 to any request that doesn't have
        // an `Upgrade` header.
        if !req.headers().contains_key(UPGRADE) {
            let mut res = Response::new(Body::from("Hello World"));
            *res.status_mut() = StatusCode::OK;
            return Box::pin(async { Ok(res) });
        }

        // Everything is ready for upgrade //

        // convert to tungstenite request
        let (parts, _) = req.into_parts();
        let r = TungsteniteRequest::from_parts(parts, ());

        // convert to tungstenite response
        let (parts, _) = tungstenite_create_response(&r).unwrap().into_parts();

        // convert back to hyper request
        let (req_parts, _) = r.into_parts();
        let mut r = Request::from_parts(req_parts, Body::empty());

        let tx = self.tx.take().unwrap();
        tokio::task::spawn(async move {
            match hyper::upgrade::on(&mut r).await {
                Ok(upgraded) => {
                    let tcp_stream = match upgraded.downcast::<AddrStream>() {
                        Ok(addr) => addr.io.into_inner(),
                        Err(_) => return,
                    };

                    let adapter = TokioAdapter::new(tcp_stream);
                    let ws = WebSocketStream::from_raw_socket(adapter, Role::Server, None).await;
                    let query = match r.uri().query() {
                        Some(s) => s.to_string(),
                        _ => String::with_capacity(0),
                    };

                    //TODO: grab the client addr, check X-Forwarded-For for forwarded connections through proxies
                    let ctx = ConnectionContext::new(
                        None,
                        r.headers().clone(),
                        query,
                        r.uri().path().to_owned(),
                    );
                    if let Err(e) = tx.send(Connection::WebSocket((ws, ctx))) {
                        tracing::error!("Unable to send upgraded Connection to engine.");
                        let frame = CloseFrame {
                            code: CloseCode::Error,
                            reason: Default::default(),
                        };

                        match e {
                            Connection::WebSocket((mut ws, ctx)) => {
                                let _ = ws.close(Some(frame)).await;
                                drop(ctx);
                                drop(ws);
                            }
                            _ => unreachable!("We should never get here"),
                        }
                    }
                }
                Err(e) => tracing::error!("upgrade failed: {}", e),
            }
        });

        Box::pin(async { Ok(Response::from_parts(parts, Body::empty())) })
    }
}

pub(crate) struct HttpService {
    engine: crate::Server,
}

impl HttpService {
    pub(crate) fn new(engine: crate::Server) -> Self {
        Self { engine }
    }

    pub(crate) async fn serve(self, addrs: SocketAddr) -> Server<AddrIncoming, HttpService> {
        let incoming = AddrIncoming::bind(&addrs).unwrap();
        hyper::server::Builder::new(incoming, Http::new())
            .http2_max_concurrent_streams(1024u32)
            .serve(self)
    }
}

impl Service<&AddrStream> for HttpService {
    type Response = Router;
    type Error = hyper::Error;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: &AddrStream) -> Self::Future {
        let (tx, rx) = channel::<Connection>();
        let mut engine = self.engine.clone();
        let paths = engine.endpoints.get_paths();
        tokio::spawn(async move {
            match rx.await {
                Ok(Connection::WebSocket((ws, ctx))) => engine.handle_new_session(ws, ctx).await,
                Ok(Connection::Metrics(_)) => (),
                Err(_) => (),
            };
        });

        Box::pin(async move { Ok(Router { tx: Some(tx), paths }) })
    }
}

#[allow(dead_code)]
enum Connection {
    WebSocket((WebSocketStream<TokioAdapter<TcpStream>>, ConnectionContext)),
    Metrics(Request<Body>),
}
