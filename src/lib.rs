use anyhow::Result;
use std::{fs::File, io::Read};

// Import the Spin HTTP objects from the generated bindings.
use spin_http::{Request, Response};

// Generate Rust bindings for interface defined in spin-http.wit file
wit_bindgen_rust::export!("spin-http.wit");

/// The default value for the cache control header.
const CACHE_CONTROL_DEFAULT_VALUE: &str = "max-age=31536000";
/// Environment variable for the cache configuration.
const CACHE_CONTROL_ENV: &str = "CACHE_CONTROL";
/// Path prefix.
const PATH_PREFIX_ENV: &str = "PATH_PREFIX";

/// The Spin HTTP component.
struct SpinHttp;

impl spin_http::SpinHttp for SpinHttp {
    /// Implement the `handler` entrypoint for Spin HTTP components.
    fn handler(req: Request) -> Response {
        let headers = Some(Self::headers(&req.uri));
        let path =
            Self::get_header("PATH_INFO", &req.headers).expect("PATH_INFO header must be set");
        let (body, status) = match Self::read(&path) {
            Ok(b) => (Some(b), 200),
            Err(e) => {
                eprintln!("Cannot read file: {:?}", e);
                (Some("404 Not Found.".as_bytes().to_vec()), 404)
            }
        };

        Response {
            status,
            headers,
            body,
        }
    }
}

impl SpinHttp {
    /// Open the file given its path and return its content and content type header.
    fn read(path: &str) -> Result<Vec<u8>> {
        let path = match std::env::var(PATH_PREFIX_ENV) {
            Ok(prefix) => format!("{}{}", prefix, &path[1..path.len()]),
            Err(_) => path.to_string(),
        };

        let mut file = File::open(path)?;
        let mut buf = vec![];
        file.read_to_end(&mut buf)?;

        Ok(buf)
    }

    /// Return the media type of the file based on the path.
    fn mime(uri: &str) -> Option<String> {
        let guess = mime_guess::from_path(uri);
        guess.first().map(|m| m.to_string())
    }

    /// The response headers.
    fn headers(uri: &str) -> Vec<(String, String)> {
        let mut headers = vec![];
        let cache_control = match std::env::var(CACHE_CONTROL_ENV) {
            Ok(c) => c,
            Err(_) => CACHE_CONTROL_DEFAULT_VALUE.to_string(),
        };
        headers.push((http::header::CACHE_CONTROL.to_string(), cache_control));

        if let Some(m) = Self::mime(uri) {
            headers.push((http::header::CONTENT_TYPE.to_string(), m));
        };

        headers
    }

    /// Get the value of a header.
    fn get_header(key: &str, headers: &[(String, String)]) -> Option<String> {
        let mut res: Option<String> = None;
        for (k, v) in headers {
            if k.to_lowercase() == key.to_lowercase() {
                res = Some(v.clone());
            }
        }

        res
    }
}
