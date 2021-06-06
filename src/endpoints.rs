use crate::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};

use std::collections::HashMap;
use tokio::sync::broadcast::{channel, Sender};

#[derive(Debug, Clone)]
pub(crate) enum Dispatch {
    Open(WebSocketSession),
    Message(WebSocketSession, WebSocketMessage),
    Close(WebSocketSession, WebSocketMessage),
}

#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    channels: HashMap<&'static str, Sender<Dispatch>>,
}

impl Endpoints {
    #[tracing::instrument(level = "trace")]
    pub(crate) async fn contains_path(&self, key: &str) -> bool {
        self.channels.contains_key(key)
    }

    pub(crate) fn insert(
        &mut self,
        key: &'static str,
        mut handler: Box<impl WebSocketHandler + 'static>,
    ) {
        let (tx, mut rx) = channel::<Dispatch>(128);

        let key2 = key.clone();
        self.channels.insert(key, tx);

        let f = async move {
            loop {
                match rx.recv().await {
                    Ok(message) => match message {
                        Dispatch::Open(session) => handler.on_open(&session).await,
                        Dispatch::Message(session, msg) => handler.on_message(&session, msg).await,
                        Dispatch::Close(session, msg) => handler.on_close(&session, msg).await,
                    },
                    Err(e) => tracing::error!("handler endpoint: {}: {:?}", key2, e),
                }
            }
        };

        tokio::spawn(f);
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_open(&self, session: &WebSocketSession) {
        self.channels
            .get(session.context().path().as_str())
            .and_then(|tx| match tx.send(Dispatch::Open(session.clone())) {
                Ok(t) => Some(t),
                Err(e) => {
                    tracing::error!("{:?}", e);
                    None
                }
            });
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_message(&self, session: &WebSocketSession, msg: WebSocketMessage) {
        self.channels
            .get(session.context().path().as_str())
            .and_then(
                |tx| match tx.send(Dispatch::Message(session.clone(), msg)) {
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
        self.channels
            .get(session.context().path().as_str())
            .and_then(|tx| match tx.send(Dispatch::Close(session.clone(), msg)) {
                Ok(t) => Some(t),
                Err(e) => {
                    tracing::error!("{:?}", e);
                    None
                }
            });
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
    #[tokio::test]
    async fn endpoint_contains_key() {
        let e = crate::Endpoints::default();
        let key = "ws";

        assert_eq!(e.contains_path(key).await, false);
    }
}
