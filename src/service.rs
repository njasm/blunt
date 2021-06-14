use crate::websocket::ConnectionContext;
use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::tungstenite::handshake::server::create_response as tungstenite_create_response;
use async_tungstenite::tungstenite::handshake::server::Request as TungsteniteRequest;
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use async_tungstenite::tungstenite::protocol::{CloseFrame, Role};
use async_tungstenite::WebSocketStream;
use hyper::server::conn::{AddrIncoming, AddrStream, Http};
use hyper::service::Service;
use hyper::{Body, Request, Response, Server, StatusCode};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;

use tokio::sync::broadcast::{channel, Sender};
use std::collections::HashMap;
use crate::endpoints::ForPath;
use std::sync::Arc;

pub(crate) struct Router {
    tx: Sender<Connection>,
    paths: HashMap<&'static str, ForPath>,
}

impl Router {
    /// Upgrade to a websocket connection
    fn upgrade(&mut self, mut req: Request<Body>) {
        let tx = self.tx.clone();
        tokio::task::spawn(async move {
            match hyper::upgrade::on(&mut req).await {
                Ok(upgraded) => {
                    let tcp_stream = match upgraded.downcast::<AddrStream>() {
                        Ok(addr) => addr.io.into_inner(),
                        Err(_) => return,
                    };

                    let adapter = TokioAdapter::new(tcp_stream);
                    let ws = WebSocketStream::from_raw_socket(adapter, Role::Server, None).await;
                    let query = match req.uri().query() {
                        Some(s) => s.to_string(),
                        _ => String::with_capacity(0),
                    };

                    //TODO: grab the client addr, check X-Forwarded-For for forwarded connections through proxies
                    let ctx = ConnectionContext::new(
                        None,
                        req.headers().clone(),
                        query,
                        req.uri().path().to_owned(),
                    );
                    if let Err(e) = tx.send(Connection::WebSocket(WebSocketConnWrapper::new(ws, ctx))) {
                        tracing::error!("Unable to send upgraded Connection to engine.");
                        let frame = CloseFrame {
                            code: CloseCode::Error,
                            reason: Default::default(),
                        };

                        match e.0 {
                            Connection::WebSocket(wrapper) => {
                                let(mut ws, ctx) = wrapper.into_parts();
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
    }
}

impl Service<Request<Body>> for Router {
    type Response = Response<Body>;
    type Error = hyper::Error;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output=Result<Self::Response, Self::Error>> + Send>>;

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
        let req_path = req.uri().path();
        return match self.paths.get(req_path) {
            None => {
                tracing::warn!("Not found path: {}", req_path);
                let mut res = Response::new(Body::empty());
                *res.status_mut() = StatusCode::NOT_FOUND;
                Box::pin(async { Ok(res) })
            }
            Some(v) => match *v {
                ForPath::Socket => {
                    // convert to tungstenite request
                    let (parts, _) = req.into_parts();
                    let r = TungsteniteRequest::from_parts(parts, ());
                    let (parts, _) = match tungstenite_create_response(&r) {
                        Ok(resp) => resp.into_parts(),
                        Err(e) => {
                            tracing::error!("{:?}:{}", e, e);
                            return Box::pin(async {
                                Ok(Response::builder()
                                    .status(StatusCode::BAD_REQUEST)
                                    .body(Body::empty())
                                    .unwrap()
                                )
                            });
                        }
                    };

                    // convert back to hyper request
                    let (req_parts, _) = r.into_parts();
                    let r = Request::from_parts(req_parts, Body::empty());
                    self.upgrade(r);

                    Box::pin(async { Ok(Response::from_parts(parts, Body::empty())) })
                }
                ForPath::Web => {
                    let (tx, mut rx) = channel(8);
                    if let Err(_e) = self.tx.send(Connection::Metrics(WebConnWrapper::new(req, tx))) {
                        tracing::error!("Unable to send web Connection to engine");
                    }

                    Box::pin(async move {
                        let result = match rx.recv().await {
                            Ok(t) => match Arc::try_unwrap(t) {
                                Ok(r) => r.unwrap(),
                                _ => panic!("Ups 2")
                            },
                            Err(e) => {
                                tracing::error!("Error ForPath::Web responding: {:?}", e);
                                panic!("Ups")
                            }
                        };

                        Ok(result)
                    })
                }
            }
        };

        // check here if we have any of this endpoints registered
        // if !self.paths.iter().any(|p| (p.0) == req.uri().path()) {
        //     tracing::warn!("Not found path: {}", req.uri().path());
        //     let mut res = Response::new(Body::empty());
        //     *res.status_mut() = StatusCode::NOT_FOUND;
        //     return Box::pin(async { Ok(res) });
        // }

        // Everything is ready for ws upgrade //

        // convert to tungstenite request
        // let (parts, _) = req.into_parts();
        // let r = TungsteniteRequest::from_parts(parts, ());
        // let (parts, _) = match tungstenite_create_response(&r) {
        //     Ok(resp) => resp.into_parts(),
        //     Err(e) => {
        //         tracing::error!("{:?}:{}", e, e);
        //         return Box::pin(async {
        //             Ok(Response::builder()
        //                 .status(StatusCode::BAD_REQUEST)
        //                 .body(Body::empty())
        //                 .unwrap()
        //             )
        //         })
        //     }
        // };
        //
        // // convert back to hyper request
        // let (req_parts, _) = r.into_parts();
        // let r = Request::from_parts(req_parts, Body::empty());
        // self.upgrade(r);
        //
        // Box::pin(async { Ok(Response::from_parts(parts, Body::empty())) })
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
    type Future = Pin<Box<dyn Future<Output=Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: &AddrStream) -> Self::Future {
        let (tx, mut rx) = channel::<Connection>(8);
        let mut engine = self.engine.clone();
        let paths = engine.endpoints.get_paths();
        tokio::spawn(async move {
            'out: loop {
                match rx.recv().await {
                    Ok(Connection::WebSocket(wrapper)) => {
                        let (ws, ctx) = wrapper.into_parts();
                        engine.handle_new_session(ws, ctx).await
                    },
                    Ok(Connection::Metrics(wrapper)) => {
                        let (request, channel) = wrapper.into_parts();
                        let result = engine.handle_web_request(std::sync::Arc::new(request)).await;
                        match channel.send(result) {
                            Ok(_size) => (),
                            Err(e) => {
                                tracing::error!("Send Error!");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error Receiving from Router: {:?}", e);
                        break 'out;
                    },
                };
            }

        });

        Box::pin(async move {
            Ok(Router {
                tx,
                paths,
            })
        })
    }
}

#[derive(Clone)]
enum Connection {
    WebSocket(WebSocketConnWrapper),
    Metrics(WebConnWrapper),
}

#[derive(Clone)]
struct WebSocketConnWrapper {
    ws: Arc<WebSocketStream<TokioAdapter<TcpStream>>>,
    ctx: ConnectionContext
}

impl WebSocketConnWrapper {
    fn new(ws: WebSocketStream<TokioAdapter<TcpStream>>, ctx: ConnectionContext) -> Self {
        Self {
            ws: Arc::new(ws),
            ctx
        }
    }

    fn into_parts(self) -> (WebSocketStream<TokioAdapter<TcpStream>>, ConnectionContext) {
        let ws = Arc::try_unwrap(self.ws).unwrap();
        (ws, self.ctx)
    }
}

#[derive(Clone)]
struct WebConnWrapper {
    request: Arc<Request<Body>>,
    channel: Sender<Arc<hyper::Result<Response<Body>>>>
}

impl WebConnWrapper {
    fn new(request: Request<Body>, channel: Sender<Arc<hyper::Result<Response<Body>>>>) -> Self {
        Self { request: Arc::new(request), channel }
    }

    fn into_parts(self) -> (Request<Body>, Sender<Arc<hyper::Result<Response<Body>>>>) {
        let request = Arc::try_unwrap(self.request).unwrap();
        (request, self.channel)
    }
}