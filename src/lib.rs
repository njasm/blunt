mod service;

use crate::builder::Builder;
use crate::endpoints::Endpoints;
use crate::websocket::WebSocketSession;

use std::collections::HashMap;
use tokio::sync::oneshot::Sender;

pub mod builder;
pub mod server;
pub mod endpoints;
pub mod webhandler;
pub mod websocket;

pub use async_trait::async_trait;
pub use async_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
pub use hyper::{Body, Request, Response, Result, StatusCode};
use uuid::Uuid;

pub fn builder() -> Builder {
    Builder::new()
}

pub mod handler {
    use crate::server::AppContext;

    pub trait Handler {
        fn init(&mut self, app: AppContext);
    }
}


/// Internal Message Type to work with the async Task
/// responsible to handle the Web Socket Sessions Collection
#[derive(Debug)]
pub(crate) enum SessionMessage {
    /// Add a new Web Socket Session to the Collection
    Add(WebSocketSession),
    /// Remove the Web Socket Session identified by the Uuid
    Remove(Uuid, Option<Sender<Option<WebSocketSession>>>),
    /// Returns a cloned Web Socket Session identified by the Uuid
    Get(Uuid, Option<Sender<Option<WebSocketSession>>>),
    Metrics(Sender<MetricsMetadata>),
}

#[derive(Clone, Debug)]
pub struct MetricsMetadata {
    total_sessions: usize,
    path_counter: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    fn test_send_sync<T: Send + Sync>(_server: &T) {}

    #[tokio::test]
    async fn test_server_is_send_and_sync() {
        let endpoints = crate::Endpoints::default();
        let server = crate::Server::new(endpoints);

        test_send_sync(&server);
        assert!(true);
    }
}
