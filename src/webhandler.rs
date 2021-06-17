use hyper::Result;
use hyper::{Body, Request, Response};
use std::fmt::Debug;

use crate::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait WebHandler: Sync + Send + Debug {
    async fn handle(&mut self, request: Request<Body>) -> Arc<Result<Response<Body>>>;
}
