use crate::webhandler::WebHandler;
use crate::websocket::WebSocketHandler;
use crate::{Endpoints, server::Server};

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
        mut self,
        path: &'static str,
        handler: impl WebSocketHandler + 'static,
    ) -> Builder {
        self.endpoints
            .insert_websocket_handler(path, Box::new(handler));

        Builder { ..self }
    }

    pub fn for_web_path(
        mut self,
        path: &'static str,
        handler: impl WebHandler + 'static,
    ) -> Builder {
        self.endpoints.insert_web_handler(path, Box::new(handler));

        Builder { ..self }
    }

    pub fn build(self) -> Server {
        Server::new(self.endpoints)
    }
}
