use anyhow::Result;
use http::header::{ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE};
use std::{fs::File, io::Read};

// Import the Spin HTTP objects from the generated bindings.
use spin_http::{Request, Response};

// Generate Rust bindings for interface defined in spin-http.wit file
wit_bindgen_rust::export!("spin-http.wit");

/// The default value for the cache control header.
const CACHE_CONTROL_DEFAULT_VALUE: &str = "max-age=31536000";
/// Environment variable for the cache configuration.
const CACHE_CONTROL_ENV: &str = "CACHE_CONTROL";
/// Environment variable for Accept-Encoding header.
//const ACCEPT_ENCODING: &str = "HTTP_ACCEPT_ENCODING";
/// Path prefix.
const PATH_PREFIX_ENV: &str = "PATH_PREFIX";
/// Brotli compression level 1-11.
///
/// 5-6 is considered the balance between compression time and
/// resulting size. 3 is faster, but doesn't compress as much.
const BROTLI_LEVEL: u32 = 3;
/// Brotli content encoding identifier
const BROTLI_ENCODING: &str = "br";

/// Common Content Encodings
#[derive(PartialEq)]
enum ContentEncoding {
    Brotli,
    //Deflate, // Could use flate2 for this
    //Gzip,    // Could use flate2 for this
    None,
}

impl ContentEncoding {
    /// Return the best ContentEncoding
    ///
    /// Currently, Brotli is the only one we care about. For the
    /// rest, we don't encode.
    fn best_encoding(req: &Request) -> ContentEncoding {
        let accept_encoding = req.headers.iter().find(|(k, v)| *k == ACCEPT_ENCODING.to_string());
        match accept_encoding {
            None => ContentEncoding::None,
            Some((_, encodings)) => {
                match encodings.split(",").find(|s| {
                    let encoding = s.trim().to_lowercase();
                    eprintln!("Encoding {}", encoding);
                    encoding == BROTLI_ENCODING
                }) {
                    Some(_) => ContentEncoding::Brotli,
                    None => ContentEncoding::None,
                }
            }
        }
    }
}

/// The Spin HTTP component.
struct SpinHttp;

impl spin_http::SpinHttp for SpinHttp {
    /// Implement the `handler` entrypoint for Spin HTTP components.
    fn handler(req: Request) -> Response {
        let encoding = ContentEncoding::best_encoding(&req);
        let path =
            Self::get_header("PATH_INFO", &req.headers).expect("PATH_INFO header must be set");
        let (body, status) = match Self::read(&path, &encoding) {
            Ok(b) => (Some(b), 200),
            Err(e) => {
                eprintln!("Cannot read file: {:?}", e);
                // Error headers are different than success headers
                return Response {
                    status: 404,
                    headers: Some(Self::error_headers()),
                    body: Some("Not Found".as_bytes().to_vec()),
                };
            }
        };
        let headers = Some(Self::headers(&req.uri, encoding));
        Response {
            headers,
            body,
            status,
        }
    }
}

impl SpinHttp {
    /// Open the file given its path and return its content and content type header.
    fn read(path: &str, encoding: &ContentEncoding) -> Result<Vec<u8>> {
        let path = match std::env::var(PATH_PREFIX_ENV) {
            Ok(prefix) => format!("{}{}", prefix, &path[1..path.len()]),
            Err(_) => path.to_string(),
        };

        let mut file = File::open(path)?;
        let mut buf = vec![];
        match encoding {
            ContentEncoding::Brotli => {
                let mut r = brotli::CompressorReader::new(file, 4096, BROTLI_LEVEL, 20);
                r.read_to_end(&mut buf)
            }
            _ => file.read_to_end(&mut buf),
        }?;

        Ok(buf)
    }

    /// Return the media type of the file based on the path.
    fn mime(uri: &str) -> Option<String> {
        let guess = mime_guess::from_path(uri);
        guess.first().map(|m| m.to_string())
    }

    /// The response headers.
    fn headers(uri: &str, encoding: ContentEncoding) -> Vec<(String, String)> {
        let mut headers = vec![];
        let cache_control = match std::env::var(CACHE_CONTROL_ENV) {
            Ok(c) => c,
            Err(_) => CACHE_CONTROL_DEFAULT_VALUE.to_string(),
        };
        headers.push((CACHE_CONTROL.to_string(), cache_control));
        if encoding == ContentEncoding::Brotli {
            headers.push((CONTENT_ENCODING.to_string(), BROTLI_ENCODING.to_string()));
        }

        if let Some(m) = Self::mime(uri) {
            headers.push((CONTENT_TYPE.to_string(), m));
        };

        headers
    }

    /// Create headers for a plain text error message.
    fn error_headers() -> Vec<(String, String)> {
        vec![(CONTENT_TYPE.to_string(), "text/plain".to_string())]
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
