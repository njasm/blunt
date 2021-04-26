use crate::websocket::{WebSocketHandler, WebSocketMessage, WebSocketSession};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    inner: Arc<RwLock<HashMap<String, Box<dyn WebSocketHandler>>>>,
}

impl Endpoints {
    #[tracing::instrument(level = "trace")]
    pub(crate) async fn contains_path(&self, key: &str) -> bool {
        self.inner.read().await.contains_key(key)
    }

    pub(crate) async fn insert(
        &mut self,
        key: impl Into<String>,
        handler: Box<impl WebSocketHandler + 'static>,
    ) -> Option<Box<dyn WebSocketHandler>> {
        self.inner.write().await.insert(key.into(), handler)
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_open(&self, session: &WebSocketSession) {
        let mut lock = self.inner.write().await;
        if let Some(h) = lock.get_mut(session.context().path().as_str()) {
            h.on_open(session).await;
        }
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_message(&self, session: &WebSocketSession, msg: WebSocketMessage) {
        let mut lock = self.inner.write().await;
        if let Some(h) = lock.get_mut(session.context().path().as_str()) {
            h.on_message(session, msg).await;
        }
    }

    #[tracing::instrument(level = "trace")]
    pub(crate) async fn on_close(&self, session: &WebSocketSession, msg: WebSocketMessage) {
        let mut lock = self.inner.write().await;
        if let Some(h) = lock.get_mut(session.context().path().as_str()) {
            h.on_close(session, msg).await;
        }
    }
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
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
