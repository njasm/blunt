use crate::websocket::ConnectionContext;
use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::tungstenite::handshake::server::create_response as tungstenite_create_response;
use async_tungstenite::tungstenite::handshake::server::Request as TungsteniteRequest;
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use async_tungstenite::tungstenite::protocol::{CloseFrame, Role};
use async_tungstenite::WebSocketStream;
use hyper::server::conn::{AddrIncoming, AddrStream, Http};
use hyper::service::Service;
use hyper::Server as HyperServer;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;

use crate::endpoints::ForPath;
use crate::rt::mpsc::{unbounded_channel, UnboundedSender};
use crate::server::Server;
use crate::{spawn, Body, Request, Response, StatusCode};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct Router {
    tx: UnboundedSender<RequestType>,
    paths: HashMap<&'static str, ForPath>,
}

impl Router {
    /// Upgrade to a websocket connection
    fn upgrade(&mut self, mut req: Request<Body>) {
        let tx = self.tx.clone();
        spawn(async move {
            match hyper::upgrade::on(&mut req).await {
                Ok(upgraded) => {
                    let tcp_stream = match upgraded.downcast::<AddrStream>() {
                        Ok(addr) => addr.io.into_inner(),
                        Err(_) => return,
                    };

                    let adapter = TokioAdapter::new(tcp_stream);
                    let ws = WebSocketStream::from_raw_socket(adapter, Role::Server, None).await;
                    let (parts, _) = req.into_parts();
                    let ctx = ConnectionContext::from_parts(parts);
                    if let Err(e) = tx.send(RequestType::Socket(WebSocketConnWrapper::new(ws, ctx)))
                    {
                        tracing::error!("Unable to send upgraded Connection to engine.");
                        let frame = CloseFrame {
                            code: CloseCode::Error,
                            reason: Default::default(),
                        };

                        if let RequestType::Socket(wrapper) = e.0 {
                            let (mut ws, ctx) = wrapper.into_parts();
                            let _ = ws.close(Some(frame)).await;
                            drop(ctx);
                            drop(ws);
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
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // favicon.ico
        if req.uri().path().to_lowercase() == "/favicon.ico" {
            return Box::pin(async {
                Ok(Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Body::empty())
                    .unwrap())
            });
        }

        // check here if we have any of this endpoints registered
        let req_path = req.uri().path();
        return match self.paths.get(req_path) {
            None => {
                tracing::warn!("Not found path: {}", req_path);
                Box::pin(async {
                    Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap())
                })
            }
            Some(v) => match *v {
                ForPath::Socket => {
                    // convert to tungstenite request
                    let (parts, _) = req.into_parts();
                    let r = TungsteniteRequest::from_parts(parts, ());
                    let (parts, _) = match tungstenite_create_response(&r) {
                        Ok(resp) => resp.into_parts(),
                        Err(e) => {
                            tracing::error!("{:?}", e);
                            return Box::pin(async {
                                Ok(Response::builder()
                                    .status(StatusCode::BAD_REQUEST)
                                    .body(Body::empty())
                                    .unwrap())
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
                    let (tx, mut rx) = unbounded_channel();
                    if let Err(_e) = self.tx.send(RequestType::Web(WebConnWrapper::new(req, tx))) {
                        tracing::error!("Unable to send web Connection to engine");
                        return Box::pin(async {
                            Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::empty())
                                .unwrap())
                        });
                    }

                    Box::pin(async move {
                        let result = match rx.recv().await {
                            Some(t) => match Arc::try_unwrap(t) {
                                Ok(r) => r.unwrap(),
                                _ => unreachable!("Ups, we should have only one ref to this Arc"),
                            },
                            None => Response::builder()
                                .status(StatusCode::NO_CONTENT)
                                .body(Body::empty())
                                .unwrap(),
                        };

                        Ok(result)
                    })
                }
            },
        };
    }
}

pub(crate) struct HttpService {
    engine: crate::server::Server,
    registered_paths: HashMap<&'static str, ForPath>,
}

impl HttpService {
    pub(crate) fn new(engine: Server) -> Self {
        let paths = engine.endpoints.get_paths();
        Self {
            engine,
            registered_paths: paths,
        }
    }

    pub(crate) async fn serve(self, addrs: SocketAddr) -> HyperServer<AddrIncoming, HttpService> {
        let incoming = AddrIncoming::bind(&addrs).unwrap();
        hyper::server::Builder::new(incoming, Http::new())
            //.http2_max_concurrent_streams(1024u32)
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
        let tx = self.engine.service_tx.clone();
        let paths = self.registered_paths.clone();
        Box::pin(async move { Ok(Router { tx, paths }) })
    }
}

#[derive(Clone, Debug)]
pub(crate) enum RequestType {
    Socket(WebSocketConnWrapper),
    Web(WebConnWrapper),
}

#[derive(Clone, Debug)]
pub(crate) struct WebSocketConnWrapper {
    ws: Arc<WebSocketStream<TokioAdapter<TcpStream>>>,
    ctx: ConnectionContext,
}

impl WebSocketConnWrapper {
    fn new(ws: WebSocketStream<TokioAdapter<TcpStream>>, ctx: ConnectionContext) -> Self {
        Self {
            ws: Arc::new(ws),
            ctx,
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (WebSocketStream<TokioAdapter<TcpStream>>, ConnectionContext) {
        let ws = Arc::try_unwrap(self.ws).unwrap();
        (ws, self.ctx)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WebConnWrapper {
    request: Arc<Request<Body>>,
    channel: UnboundedSender<Arc<hyper::Result<Response<Body>>>>,
}

type WebConnUnboundedSender = UnboundedSender<Arc<hyper::Result<Response<Body>>>>;

impl WebConnWrapper {
    pub(crate) fn new(request: Request<Body>, channel: WebConnUnboundedSender) -> Self {
        Self {
            request: Arc::new(request),
            channel,
        }
    }

    pub(crate) fn into_parts(self) -> (Request<Body>, WebConnUnboundedSender) {
        let request = Arc::try_unwrap(self.request).unwrap();
        (request, self.channel)
    }
}
