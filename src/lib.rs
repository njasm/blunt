mod rt;
mod service;

pub use rt::task::spawn;

use crate::builder::Builder;
use crate::endpoints::Endpoints;
use crate::websocket::WebSocketSession;

use std::collections::HashMap;
use tokio::sync::oneshot::Sender;

pub mod builder;
pub mod endpoints;
pub mod server;
pub mod webhandler;
pub mod websocket;

pub use async_trait::async_trait;
pub use async_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
pub use hyper::{Body, Request, Response, Result, StatusCode};
use uuid::Uuid;

pub fn builder() -> Builder {
    Builder::new()
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
    /// Returns the Path that a Web Socket Session is connected to
    GetPath(Uuid, Sender<Option<String>>),
    /// Returns basic metric data about global total sessions connected, and per path
    Metrics(Sender<MetricsMetadata>),
}

#[derive(Clone, Debug)]
pub struct MetricsMetadata {
    total_sessions: usize,
    path_counter: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::unbounded_channel;

    fn test_send_sync<T: Send + Sync>(_server: &T) {}

    #[tokio::test]
    async fn test_server_is_send_and_sync() {
        let endpoints = crate::Endpoints::default();
        let app_ctx_tuple = unbounded_channel();
        let server = crate::server::Server::new(endpoints, app_ctx_tuple);

        test_send_sync(&server);
        assert!(true);
    }
}
