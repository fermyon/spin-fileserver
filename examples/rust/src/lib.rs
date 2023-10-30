use {
    bindings::wasi::http::incoming_handler,
    futures::SinkExt,
    spin_sdk::{
        http::{Fields, IncomingRequest, Method, OutgoingResponse, ResponseOutparam},
        http_component,
    },
};

mod bindings {
    wit_bindgen::generate!({
        path: "../wit",
        world: "delegate",
        with: {
            "wasi:http/types@0.2.0-rc-2023-10-18": spin_sdk::wit::wasi::http::types,
            "wasi:io/streams@0.2.0-rc-2023-10-18": spin_sdk::wit::wasi::io::streams,
            "wasi:io/poll@0.2.0-rc-2023-10-18": spin_sdk::wit::wasi::io,
        }
    });
}

#[http_component]
async fn handle_request(request: IncomingRequest, response_out: ResponseOutparam) {
    match (request.method(), request.path_with_query().as_deref()) {
        (Method::Get, Some("/hello")) => {
            let response = OutgoingResponse::new(
                200,
                &Fields::new(&[("content-type".to_string(), b"text/plain".to_vec())]),
            );

            let mut body = response.take_body();

            response_out.set(response);

            if let Err(e) = body.send(b"Hello, world!".to_vec()).await {
                eprintln!("Error sending payload: {e}");
            }
        }

        (Method::Get, _) => {
            // Delegate to spin-fileserver component
            incoming_handler::handle(request, response_out.into_inner())
        }

        _ => {
            response_out.set(OutgoingResponse::new(405, &Fields::new(&[])));
        }
    }
}
