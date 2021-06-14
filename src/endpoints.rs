use crate::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};

use std::collections::HashMap;
use tokio::sync::broadcast::{channel, Sender};
use crate::webhandler::WebHandler;
use hyper::{Request, Body, Result, Response};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) enum Dispatch {
    Web((Arc<Request<Body>>, Sender<Arc<Result<Response<Body>>>>))
}

#[derive(Debug, Clone)]
pub(crate) enum WebSocketDispatch {
    Open(WebSocketSession),
    Message(WebSocketSession, WebSocketMessage),
    Close(WebSocketSession, WebSocketMessage),
}

#[derive(Debug, Clone)]
pub(crate) enum ForPath {
    Socket,
    Web
}

#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    ws_channels: HashMap<&'static str, Sender<WebSocketDispatch>>,
    web_channels: HashMap<&'static str, Sender<Dispatch>>,
}

impl Endpoints {
    pub(crate) fn get_paths(&self) -> HashMap<&'static str, ForPath> {
        #[allow(clippy::map_clone)]
        //self.ws_channels.keys().clone().map(|k| *k).collect()

        let mut result = HashMap::with_capacity(self.ws_channels.len() + self.web_channels.len());
        self.ws_channels.iter()
            .for_each(|(k, _v)| {
                let _ = result.insert((*k).clone(), ForPath::Socket);
            });

        self.web_channels.iter()
            .for_each(|(k, _v)| {
                let _ = result.insert((*k).clone(), ForPath::Web);
            });

        result
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn contains_path(&self, key: &str) -> bool {
        self.ws_channels.contains_key(key)
    }

    pub(crate) fn insert_websocket_handler(
        &mut self,
        key: &'static str,
        mut handler: Box<impl WebSocketHandler + 'static>,
    ) {
        let (tx, mut rx) = channel::<WebSocketDispatch>(128);
        let key2 = key.to_string();
        self.ws_channels.insert(key, tx);

        let f = async move {
            loop {
                match rx.recv().await {
                    Ok(message) => match message {
                        WebSocketDispatch::Open(session) => handler.on_open(&session).await,
                        WebSocketDispatch::Message(session, msg) => handler.on_message(&session, msg).await,
                        WebSocketDispatch::Close(session, msg) => handler.on_close(&session, msg).await,
                    },
                    Err(e) => tracing::error!("handler endpoint: {}: {:?}", key2, e),
                }
            }
        };

        tokio::spawn(f);
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_open(&self, session: &WebSocketSession) {
        self.ws_channels
            .get(session.context().path().as_str())
            .and_then(|tx| match tx.send(WebSocketDispatch::Open(session.clone())) {
                Ok(t) => Some(t),
                Err(e) => {
                    tracing::error!("{:?}", e);
                    None
                }
            });
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_message(&self, session: &WebSocketSession, msg: WebSocketMessage) {
        self.ws_channels
            .get(session.context().path().as_str())
            .and_then(
                |tx| match tx.send(WebSocketDispatch::Message(session.clone(), msg)) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        tracing::error!("{:?}", e);
                        None
                    }
                },
            );
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_close(&self, session: &WebSocketSession, msg: WebSocketMessage) {
        self.ws_channels
            .get(session.context().path().as_str())
            .and_then(|tx| match tx.send(WebSocketDispatch::Close(session.clone(), msg)) {
                Ok(t) => Some(t),
                Err(e) => {
                    tracing::error!("{:?}", e);
                    None
                }
            });
    }

    pub(crate) fn insert_web_handler(
        &mut self,
        key: &'static str,
        mut handler: Box<impl WebHandler + 'static>,
    ) {
        let (tx, mut rx) = channel::<Dispatch>(123);
        let key2 = key.to_string();
        self.web_channels.insert(key, tx);

        let f = async move {
            use tokio::sync::broadcast::error::RecvError;
            'out: loop {
                match rx.recv().await {
                    Ok(message) => match message {
                        Dispatch::Web((request, resp_channel)) => {
                            if let Err(_) = resp_channel.send(handler.handle(request).await) {
                                tracing::error!("Unable to dispatch Web request response from handler");
                            }
                        }
                    },
                    Err(RecvError::Closed) => {
                        tracing::error!("All senders dropped for handler endpoint: {}", key2);
                        break 'out;
                    }
                    Err(RecvError::Lagged(how_much)) => {
                        tracing::error!("Receiver Lagged {} messages for handler endpoint: {}", how_much, key2);
                        break 'out;
                    },
                }
            }
        };

        tokio::spawn(f);
    }

    pub(crate) async fn handle_web_request(&mut self, request: Arc<Request<Body>>) -> Arc<Result<Response<Body>>> {
        let result = self.web_channels
            .get(request.uri().path())
            .and_then(|tx| {
                let (inner_tx, rx) = channel::<Arc<Result<Response<Body>>>>(1);
                match tx.send(Dispatch::Web((request.clone(), inner_tx))) {
                    Ok(_t) => (),
                    Err(e) => {
                        tracing::error!("{:?}", e);
                    }
                };

               Some(rx)
            });

        match result {
            Some(mut r) => match r.recv().await {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("Unable to receive web handler response: {:?}", e);
                    panic!("Ups 5")
                }
            },
            None => panic!("Ups 4")
        }
    }
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            ws_channels: HashMap::new(),
            web_channels: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};

    #[derive(Debug, Default)]
    struct Handler;

    #[crate::async_trait]
    impl WebSocketHandler for Handler {
        async fn on_open(&mut self, _ws: &WebSocketSession) {}
        async fn on_message(&mut self, _ws: &WebSocketSession, _msg: WebSocketMessage) {}
        async fn on_close(&mut self, _ws: &WebSocketSession, _msg: WebSocketMessage) {}
    }

    #[tokio::test]
    async fn endpoint_contains_key() {
        let mut e = crate::Endpoints::default();
        let key = "ws";

        assert_eq!(e.contains_path(key).await, false);

        let h = Handler::default();
        e.insert_websocket_handler(key, Box::new(h));

        assert_eq!(e.contains_path(key).await, true);
    }
}
