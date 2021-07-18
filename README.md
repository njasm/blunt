Blunt
=

[<img alt="github" src="https://img.shields.io/badge/github-njasm/blunt-blueviolet?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/njasm/blunt)
[<img alt="crates.io" src="https://img.shields.io/crates/v/blunt.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/blunt)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-blunt-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/blunt)
[<img alt="build status" src="https://img.shields.io/github/workflow/status/njasm/blunt/CI/master?style=for-the-badge" height="20">](https://github.com/njasm/blunt/actions?query=branch%3Amaster)

Highly opinionated way to build asynchronous Websocket servers with Rust

### How

The world famous example, echo server
```rust
#[tokio::main]
async fn main() -> Result<()> {
    let handler = EchoServer::default();
    ::blunt::builder()
        .for_path("/echo", handler)
        .build()
        .bind("127.0.0.1:3000".parse().expect("Socket Addr"))
        .await?
    
    // now connect your clients to http://127.0.0.1:3000/echo and say something!
}

#[derive(Debug, Default)]
pub struct EchoServer;

#[blunt::async_trait]
impl WebSocketHandler for EchoServer {
    async fn on_open(&mut self, ws: &WebSocketSession) {
        info!("new connection open with id: {}", ws.id());
    }

    async fn on_message(&mut self, ws: &WebSocketSession, msg: WebSocketMessage) {
        ws.send(msg).expect("Unable to send message");
    }

    async fn on_close(&mut self, ws: &WebSocketSession, _msg: WebSocketMessage) {
        info!("connection closed for session id {}", ws.id());
    }
}
```
For more code examples please see the <a href="examples/">examples</a> folder.

### License

Tri-Licensed under either of <a href="LICENSE-APACHE-2.0">Apache License, Version
2.0</a>, <a href="LICENSE-MIT">MIT license</a> or <a href="LICENSE-MPL-2.0">MPL-2.0 license</a> at your option.
<br>

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be tri-licensed as above, without any additional terms or conditions.
