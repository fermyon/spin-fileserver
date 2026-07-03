use spin_sdk::http::{IntoResponse, Request};
use spin_sdk::http_service;

/// A simple Spin HTTP component.
#[http_service]
async fn hello_world(_req: Request) -> impl IntoResponse {
    "Hello, world!\n"
}
