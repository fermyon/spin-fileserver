use spin_test_sdk::{bindings::wasi::http, spin_test};

#[spin_test]
fn defaults_to_index() {
    let request = http::types::OutgoingRequest::new(http::types::Headers::new());
    let response = spin_test_sdk::perform_request(request);
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.body_as_string().unwrap(),
        "<html>\n\n<body>\n    This is index in root\n</body>\n\n</html>"
    );
}

#[spin_test]
fn defaults_to_index_within_directory() {
    let request = http::types::OutgoingRequest::new(http::types::Headers::new());
    request.set_path_with_query(Some("/subdirectory")).unwrap();
    let _response = spin_test_sdk::perform_request(request);
    // TODO: we need the ability to add a file to the file system within the test.
    // The file should be `subdirectory/index.html`

    // assert_eq!(response.status(), 200);
}

#[spin_test]
fn fetches_favicon_ico() {
    let favicon = std::fs::read("spin-favicon.ico").unwrap();
    let request = http::types::OutgoingRequest::new(http::types::Headers::new());
    request
        .set_path_with_query(Some("/foo/bar/favicon.ico"))
        .unwrap();
    let response = spin_test_sdk::perform_request(request);

    assert_eq!(response.status(), 200);
    assert_eq!(response.body().unwrap(), favicon);
}

#[spin_test]
fn fetches_favicon_png() {
    let favicon = std::fs::read("spin-favicon.png").unwrap();
    let request = http::types::OutgoingRequest::new(http::types::Headers::new());
    request
        .set_path_with_query(Some("/foo/bar/favicon.png"))
        .unwrap();
    let response = spin_test_sdk::perform_request(request);

    assert_eq!(response.status(), 200);
    assert_eq!(response.body().unwrap(), favicon);
}

#[spin_test]
fn not_found() {
    let request = http::types::OutgoingRequest::new(http::types::Headers::new());
    request.set_path_with_query(Some("/not-found.txt")).unwrap();
    let response = spin_test_sdk::perform_request(request);
    assert_eq!(response.status(), 404);
    assert_eq!(response.body_as_string().unwrap(), "Not Found");
}

#[spin_test]
fn fetches_file() {
    fn hex_encoded_sha256(buffer: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(buffer);
        hex::encode(hasher.finalize()).into_bytes()
    }

    let readme = std::fs::read("README.md").unwrap();
    let request = http::types::OutgoingRequest::new(http::types::Headers::new());
    request.set_path_with_query(Some("/README.md")).unwrap();
    let response = spin_test_sdk::perform_request(request);
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get(&"etag".into()),
        vec![hex_encoded_sha256(&readme)]
    );
    let body = response.body_as_string().unwrap();
    let expected_body_begin = "# Static file server for Spin applications";
    assert!(
        body.starts_with(expected_body_begin),
        "body does not start with expected content. expected '{expected_body_begin}', got '{}'",
        body.chars()
            .take(expected_body_begin.len())
            .collect::<String>()
    );
}

#[spin_test]
fn prefers_brotoli_encoding() {
    let headers = http::types::Headers::new();
    headers
        .append(
            &String::from("accept-encoding"),
            &String::from("deflate,br,gzip").into_bytes(),
        )
        .unwrap();
    let request = http::types::OutgoingRequest::new(headers);
    request.set_path_with_query(Some("/README.md")).unwrap();
    let response = spin_test_sdk::perform_request(request);
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get(&"content-encoding".into()),
        vec![String::from("br").into_bytes()]
    );
}

#[spin_test]
fn prefers_brotoli_encoding_multi_header() {
    let headers = http::types::Headers::new();
    headers
        .append(
            &String::from("accept-encoding"),
            &String::from("deflate,gzip").into_bytes(),
        )
        .unwrap();
    headers
        .append(
            &String::from("accept-encoding"),
            &String::from("br").into_bytes(),
        )
        .unwrap();
    let request = http::types::OutgoingRequest::new(headers);
    request.set_path_with_query(Some("/README.md")).unwrap();
    let response = spin_test_sdk::perform_request(request);
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get(&"content-encoding".into()),
        vec![String::from("br").into_bytes()]
    );
}
