use crate::websocket::WebSocketHandler;
use crate::{Endpoints, Server};
use tokio::task::block_in_place;
use tokio::task::spawn;

#[derive(Clone)]
pub struct Builder {
    endpoints: Endpoints,
}

impl Default for Builder {
    fn default() -> Self {
        Builder::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            endpoints: Endpoints::default(),
        }
    }

    pub fn for_path(
        self,
        path: impl Into<String>,
        handler: impl WebSocketHandler + 'static,
    ) -> Builder {
        let mut this = self.clone();
        let path = path.into();

        let rt = tokio::runtime::Handle::current();
        rt.spawn(async move {
            this.endpoints.insert(path, Box::new(handler)).await;
        });

        Builder { ..self }
    }

    pub fn build(self) -> Server {
        crate::Server::new(self.endpoints)
    }
}
