use crate::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};

use crate::webhandler::WebHandler;
use hyper::{Body, Request, Response, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tracing::error;

use crate::service::WebConnWrapper;

#[derive(Debug, Clone)]
pub(crate) enum Dispatch {
    Web(WebConnWrapper),
}

#[derive(Debug, Clone)]
pub(crate) enum WebSocketDispatch {
    Open(WebSocketSession),
    Message(WebSocketSession, WebSocketMessage),
}

#[derive(Debug, Clone)]
pub(crate) enum ForPath {
    Socket,
    Web,
}

#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    ws_channels: HashMap<&'static str, UnboundedSender<WebSocketDispatch>>,
    web_channels: HashMap<&'static str, UnboundedSender<Dispatch>>,
}

impl Endpoints {
    pub(crate) fn get_paths(&self) -> HashMap<&'static str, ForPath> {
        let mut result = HashMap::with_capacity(self.ws_channels.len() + self.web_channels.len());
        self.ws_channels.iter().for_each(|(k, _v)| {
            let _ = result.insert(<&str>::clone(k), ForPath::Socket);
        });

        self.web_channels.iter().for_each(|(k, _v)| {
            let _ = result.insert(<&str>::clone(k), ForPath::Web);
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
        let (tx, mut rx) = unbounded_channel::<WebSocketDispatch>();
        self.ws_channels.insert(key, tx);
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                match message {
                    WebSocketDispatch::Open(session) => handler.on_open(&session).await,
                    WebSocketDispatch::Message(session, msg) => {
                        handler.on_message(&session, msg).await
                    }
                };
            }
        });
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_open(&self, session: &WebSocketSession) {
        self.ws_channels
            .get(session.context().path().as_str())
            .and_then(|tx| tx.send(WebSocketDispatch::Open(session.clone())).ok());
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_message(&self, session: &WebSocketSession, msg: WebSocketMessage) {
        self.ws_channels
            .get(session.context().path().as_str())
            .and_then(|tx| {
                tx.send(WebSocketDispatch::Message(session.clone(), msg))
                    .ok()
            });
    }

    // #[tracing::instrument(level = "trace")]
    // pub(crate) async fn on_close(&self, session: &WebSocketSession, msg: WebSocketMessage) {
    //     self.ws_channels
    //         .get(session.context().path().as_str())
    //         .and_then(|tx| tx.send(WebSocketDispatch::Close(session.clone(), msg)).ok());
    // }

    pub(crate) fn insert_web_handler(
        &mut self,
        key: &'static str,
        mut handler: Box<impl WebHandler + 'static>,
    ) {
        let (tx, mut rx) = unbounded_channel::<Dispatch>();
        self.web_channels.insert(key, tx);
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                match message {
                    Dispatch::Web(wrapper) => {
                        let (request, resp_channel) = wrapper.into_parts();
                        if resp_channel.send(handler.handle(request).await).is_err() {
                            error!("Unable to dispatch Web request response from handler");
                        }
                    }
                };
            }
        });
    }

    pub(crate) async fn handle_web_request(
        &self,
        request: Request<Body>,
    ) -> Arc<Result<Response<Body>>> {
        let result = self.web_channels.get(request.uri().path()).map(|tx| {
            let (inner_tx, rx) = unbounded_channel::<Arc<Result<Response<Body>>>>();
            let wrapper = WebConnWrapper::new(request, inner_tx);
            match tx.send(Dispatch::Web(wrapper)) {
                Ok(_t) => (),
                Err(e) => error!("{:?}", e),
            };

            rx
        });

        match result {
            Some(mut r) => match r.recv().await {
                Some(t) => t,
                None => unreachable!("Ups, we should not get here. tx dropped already"),
            },
            None => unreachable!("Ups, we should not get here. there's no tx channel"),
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

    #[derive(Debug, Default, Clone)]
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
