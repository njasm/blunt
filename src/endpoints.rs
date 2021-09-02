use crate::websocket::{WebSocketHandler, WebSocketMessage};

use crate::rt::task::spawn;
use crate::webhandler::WebHandler;

use std::collections::HashMap;

use crate::rt::mpsc::{unbounded_channel, UnboundedSender};
use tracing::error;

use crate::service::WebConnWrapper;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) enum ForPath {
    Socket,
    Web,
}

pub(crate) enum Register {
    Web(Box<dyn WebHandler + 'static>),
    WebSocket(Box<dyn WebSocketHandler + 'static>),
}

pub(crate) enum HandleWeb {
    SocketOpen(Uuid),
    SocketMessage(Uuid, WebSocketMessage),
    Request(WebConnWrapper),
}

#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    channels: HashMap<&'static str, (ForPath, UnboundedSender<HandleWeb>)>,
}

impl Endpoints {
    pub(crate) fn register(&mut self, key: &'static str, handler: Register) {
        let (tx, mut rx) = unbounded_channel::<HandleWeb>();
        match handler {
            Register::Web(mut handler) => {
                self.channels.insert(key, (ForPath::Web, tx));
                spawn(async move {
                    while let Some(message) = rx.recv().await {
                        if let HandleWeb::Request(wrapper) = message {
                            let (request, resp_channel) = wrapper.into_parts();
                            if resp_channel.send(handler.handle(request).await).is_err() {
                                error!("Unable to dispatch Web request response from handler");
                            }
                        }
                    }
                });
            }
            Register::WebSocket(mut handler) => {
                self.channels.insert(key, (ForPath::Socket, tx));
                spawn(async move {
                    while let Some(message) = rx.recv().await {
                        match message {
                            HandleWeb::SocketOpen(session_id) => handler.on_open(session_id).await,
                            HandleWeb::SocketMessage(session_id, msg) => {
                                handler.on_message(session_id, msg).await
                            }
                            _ => (),
                        };
                    }
                });
            }
        }
    }

    pub(crate) fn handle(&self, path: &str, action: HandleWeb) {
        self.channels
            .get(path)
            .and_then(|value| value.1.send(action).ok());
    }

    pub(crate) fn get_paths(&self) -> HashMap<&'static str, ForPath> {
        let mut result = HashMap::with_capacity(self.channels.len());
        self.channels.iter().for_each(|(k, v)| {
            result.insert(<&str>::clone(k), v.0.clone());
        });

        result
    }
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::endpoints::Register;
    use crate::websocket::{WebSocketHandler, WebSocketMessage};
    use uuid::Uuid;

    #[derive(Debug, Default, Clone)]
    struct Handler;

    #[crate::async_trait]
    impl WebSocketHandler for Handler {
        async fn on_open(&mut self, _session_id: Uuid) {}
        async fn on_message(&mut self, _session_id: Uuid, _msg: WebSocketMessage) {}
        async fn on_close(&mut self, _session_id: Uuid, _msg: WebSocketMessage) {}
    }

    #[tokio::test]
    async fn endpoint_contains_key() {
        let mut e = crate::Endpoints::default();
        let key = "ws";

        assert_eq!(e.contains_path(key), false);

        let h = Handler::default();
        e.register(key, Register::WebSocket(Box::new(h)));

        assert_eq!(e.contains_path(key), true);
    }
}
