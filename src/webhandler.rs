use std::fmt::Debug;
use hyper::{Request, Response, Body};
use hyper::Result;

use crate::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait WebHandler: Sync + Send + Debug {
    async fn handle(&mut self, request: Arc<Request<Body>>) -> Arc<Result<Response<Body>>>;
}