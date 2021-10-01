use crate::server::{AppContext, Command};
use crate::webhandler::WebHandler;
use crate::websocket::WebSocketHandler;
use crate::{server::Server, Endpoints};

use crate::endpoints::Register;
use crate::rt::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub struct Builder {
    endpoints: Endpoints,
    channel: (UnboundedSender<Command>, UnboundedReceiver<Command>),
}

impl Default for Builder {
    fn default() -> Self {
        Builder::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        let (tx, rx) = unbounded_channel::<Command>();
        Builder {
            endpoints: Endpoints::default(),
            channel: (tx, rx),
        }
    }

    /// Register for the supplied web path the supplied WebSocketHandler
    pub fn for_path(
        mut self,
        path: &'static str,
        handler: impl WebSocketHandler + 'static,
    ) -> Builder {
        self.endpoints
            .register(path, Register::WebSocket(Box::new(handler)));

        Builder { ..self }
    }

    /// Register for the supplied web path the WebSocketHandler returned
    /// by the factory function `f`.
    pub fn for_path_with_ctor<O>(
        mut self,
        path: &'static str,
        f: impl Fn(AppContext) -> O,
    ) -> Builder
    where
        O: WebSocketHandler + 'static,
    {
        let ctx = AppContext::new(self.channel.0.clone(), path.to_string());
        let handler = f(ctx);
        self.endpoints
            .register(path, Register::WebSocket(Box::new(handler)));

        Builder { ..self }
    }

    /// Register for the supplied web path the supplied WebHandler
    pub fn for_web_path(
        mut self,
        path: &'static str,
        handler: impl WebHandler + 'static,
    ) -> Builder {
        self.endpoints
            .register(path, Register::Web(Box::new(handler)));

        Builder { ..self }
    }

    /// Register for the supplied web path the WebHandler returned
    /// by the factory function `f`.
    pub fn for_web_path_with_ctor<O>(
        mut self,
        path: &'static str,
        f: impl Fn(AppContext) -> O,
    ) -> Builder
    where
        O: WebHandler + 'static,
    {
        let ctx = AppContext::new(self.channel.0.clone(), path.to_string());
        let handler = f(ctx);
        self.endpoints
            .register(path, Register::Web(Box::new(handler)));

        Builder { ..self }
    }

    pub fn build(self) -> Server {
        Server::new(self.endpoints, self.channel)
    }
}
