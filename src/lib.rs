use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use http::{
    header::{ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE, ETAG, IF_NONE_MATCH},
    HeaderMap, StatusCode,
};
use spin_sdk::http::{not_found, Request, Response};
use std::{
    collections::hash_map::DefaultHasher,
    fs::File,
    hash::{Hash, Hasher},
    io::Read,
    path::PathBuf,
};

/// The default value for the cache control header.
const CACHE_CONTROL_DEFAULT_VALUE: &str = "max-age=60";
/// Environment variable for the cache configuration.
const CACHE_CONTROL_ENV: &str = "CACHE_CONTROL";
/// Brotli compression level 1-11.
///
/// 5-6 is considered the balance between compression time and
/// resulting size. 3 is faster, but doesn't compress as much.
const BROTLI_LEVEL: u32 = 3;
/// Brotli content encoding identifier
const BROTLI_ENCODING: &str = "br";
/// The path info header.
const PATH_INFO_HEADER: &str = "spin-path-info";
// Environment variable for the fallback path
const FALLBACK_PATH_ENV: &str = "FALLBACK_PATH";
/// Directory fallback path (trying to map `/about/` -> `/about/index.html`).
const DIRECTORY_FALLBACK_PATH: &str = "index.html";

/// Common Content Encodings
#[derive(Debug, Eq, PartialEq)]
pub enum ContentEncoding {
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
    fn best_encoding(req: &Request) -> Result<Self> {
        match req.headers().get(ACCEPT_ENCODING) {
            Some(e) => {
                match e
                    .to_str()?
                    .split(',')
                    .map(|ce| ce.trim().to_lowercase())
                    .find(|ce| ce == BROTLI_ENCODING)
                {
                    Some(_) => Ok(ContentEncoding::Brotli),
                    None => Ok(ContentEncoding::None),
                }
            }
            None => Ok(ContentEncoding::None),
        }
    }
}

#[spin_sdk::http_component]
fn serve(req: Request) -> Result<Response> {
    let enc = ContentEncoding::best_encoding(&req)?;
    let path = req
        .headers()
        .get(PATH_INFO_HEADER)
        .expect("PATH_INFO header must be set by the Spin runtime")
        .to_str()?;
    let if_none_match = req
        .headers()
        .get(IF_NONE_MATCH)
        .map(|h| h.to_str())
        .unwrap_or(Ok(""))?;

    // resolve the requested path and then try to read the file
    // None should indicate that the file does not exist after attempting fallback paths
    let body = match FileServer::resolve(path) {
        Some(path) => FileServer::read(&path, &enc).ok(),
        None => None,
    };

    let etag = FileServer::get_etag(body.clone());
    FileServer::send(body, path, enc, &etag, if_none_match)
}

struct FileServer;
impl FileServer {
    /// Resolve the request path to a file path.
    /// Returns `None` if the path does not exist.
    fn resolve(req_path: &str) -> Option<PathBuf> {
        // fallback to index.html if the path is empty
        let mut path = if req_path.is_empty() {
            PathBuf::from(DIRECTORY_FALLBACK_PATH)
        } else {
            PathBuf::from(req_path)
        };

        // if the path is a directory, try to read the fallback file relative to the directory
        if path.is_dir() {
            path.push(DIRECTORY_FALLBACK_PATH);
        }

        // if still haven't found a file, override with the user-configured fallback path
        if !path.exists() {
            if let Ok(fallback_path) = std::env::var(FALLBACK_PATH_ENV) {
                path = PathBuf::from(fallback_path);
            }
        }

        // return the path if it exists
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Open the file given its path and return its content and content type header.
    fn read(path: &PathBuf, encoding: &ContentEncoding) -> Result<Bytes> {
        let mut file =
            File::open(path).with_context(|| anyhow!("cannot open {}", path.display()))?;
        let mut buf = vec![];
        match encoding {
            ContentEncoding::Brotli => {
                let mut r = brotli::CompressorReader::new(file, 4096, BROTLI_LEVEL, 20);
                r.read_to_end(&mut buf)
            }
            _ => file.read_to_end(&mut buf),
        }?;

        Ok(buf.into())
    }

    /// Return the media type of the file based on the path.
    fn mime(uri: &str) -> Option<String> {
        let guess = mime_guess::from_path(uri);
        guess.first().map(|m| m.to_string())
    }

    fn append_headers(
        path: &str,
        enc: ContentEncoding,
        etag: &str,
        headers: &mut HeaderMap,
    ) -> Result<()> {
        let cache_control = match std::env::var(CACHE_CONTROL_ENV) {
            Ok(c) => c.try_into()?,
            Err(_) => CACHE_CONTROL_DEFAULT_VALUE.try_into()?,
        };
        headers.insert(CACHE_CONTROL, cache_control);
        headers.insert(ETAG, etag.try_into()?);

        if enc == ContentEncoding::Brotli {
            headers.insert(CONTENT_ENCODING, BROTLI_ENCODING.try_into()?);
        }

        if let Some(m) = Self::mime(path) {
            headers.insert(CONTENT_TYPE, m.try_into()?);
        };

        Ok(())
    }

    fn send(
        body: Option<Bytes>,
        path: &str,
        enc: ContentEncoding,
        etag: &str,
        if_none_match: &str,
    ) -> Result<Response> {
        let mut res = http::Response::builder();
        let headers = res
            .headers_mut()
            .ok_or(anyhow!("cannot get headers for response"))?;
        FileServer::append_headers(path, enc, etag, headers)?;

        if body.is_some() {
            if etag == if_none_match {
                return Ok(res.status(StatusCode::NOT_MODIFIED).body(None)?);
            }
            Ok(res.status(StatusCode::OK).body(body)?)
        } else {
            not_found()
        }
    }

    fn get_etag(body: Option<Bytes>) -> String {
        let mut state = DefaultHasher::new();
        body.unwrap_or_default().hash(&mut state);
        state.finish().to_string()
    }
}

#[cfg(test)]
mod tests {
    use http::header::{ACCEPT_ENCODING, IF_NONE_MATCH};

    use super::*;

    #[test]
    fn test_best_encoding_none() {
        let req = http::Request::builder()
            .uri("http://thisistest.com")
            .body(Some(bytes::Bytes::default()))
            .unwrap();
        let enc = ContentEncoding::best_encoding(&req).unwrap();
        assert_eq!(enc, ContentEncoding::None);
    }

    #[test]
    fn test_best_encoding_not_br() {
        let req = http::Request::builder()
            .uri("http://thisistest.com")
            .header(ACCEPT_ENCODING, "gzip")
            .body(Some(bytes::Bytes::default()))
            .unwrap();
        let enc = ContentEncoding::best_encoding(&req).unwrap();
        assert_eq!(enc, ContentEncoding::None);
    }

    #[test]
    fn test_best_encoding_with_br() {
        let req = http::Request::builder()
            .uri("http://thisistest.com")
            .header(ACCEPT_ENCODING, "gzip,br")
            .body(Some(bytes::Bytes::default()))
            .unwrap();
        let enc = ContentEncoding::best_encoding(&req).unwrap();
        assert_eq!(enc, ContentEncoding::Brotli);
    }

    #[test]
    fn test_serve_file_found() {
        let req = spin_http::Request {
            method: spin_http::Method::Get,
            uri: "http://thisistest.com".to_string(),
            headers: vec![(PATH_INFO_HEADER.to_string(), "./hello-test.txt".to_string())],
            params: vec![],
            body: None,
        };
        let rsp = <super::SpinHttp as spin_http::SpinHttp>::handle_http_request(req);
        assert_eq!(rsp.status, 200);
    }

    #[test]
    fn test_serve_with_etag() {
        let req = spin_http::Request {
            method: spin_http::Method::Get,
            uri: "http://thisistest.com".to_string(),
            headers: vec![
                (PATH_INFO_HEADER.to_string(), "./hello-test.txt".to_string()),
                (
                    IF_NONE_MATCH.to_string(),
                    "13946318585003701156".to_string(),
                ),
            ],
            params: vec![],
            body: None,
        };
        let rsp = <super::SpinHttp as spin_http::SpinHttp>::handle_http_request(req);
        assert_eq!(rsp.status, 304);
    }

    #[test]
    fn test_serve_with_not_matched_etag() {
        let req = spin_http::Request {
            method: spin_http::Method::Get,
            uri: "http://thisistest.com".to_string(),
            headers: vec![
                (PATH_INFO_HEADER.to_string(), "./hello-test.txt".to_string()),
                (IF_NONE_MATCH.to_string(), "".to_string()),
            ],
            params: vec![],
            body: None,
        };
        let rsp = <super::SpinHttp as spin_http::SpinHttp>::handle_http_request(req);
        assert_eq!(rsp.status, 200);
    }

    #[test]
    fn test_serve_file_not_found() {
        let req = spin_http::Request {
            method: spin_http::Method::Get,
            uri: "http://thisistest.com".to_string(),
            headers: vec![(
                PATH_INFO_HEADER.to_string(),
                "not-existent-file".to_string(),
            )],
            params: vec![],
            body: None,
        };
        let rsp = <super::SpinHttp as spin_http::SpinHttp>::handle_http_request(req);
        assert_eq!(rsp.status, 404);
    }

    #[test]
    fn test_serve_file_not_found_with_fallback_path() {
        //NOTE: this test must not run in parallel to other tests because of it's use of an environment variable
        //      hence the `--test-threads=1` in the `make test` target
        std::env::set_var(FALLBACK_PATH_ENV, "hello-test.txt");
        let req = spin_http::Request {
            method: spin_http::Method::Get,
            uri: "http://thisistest.com".to_string(),
            headers: vec![(
                PATH_INFO_HEADER.to_string(),
                "not-existent-file".to_string(),
            )],
            params: vec![],
            body: None,
        };
        let rsp = <super::SpinHttp as spin_http::SpinHttp>::handle_http_request(req);
        std::env::remove_var(FALLBACK_PATH_ENV);
        assert_eq!(rsp.status, 200);
    }

    #[test]
    fn test_serve_index() {
        // Test against path with trailing slash
        let req = spin_http::Request {
            method: spin_http::Method::Get,
            uri: "http://thisistest.com".to_string(),
            headers: vec![(PATH_INFO_HEADER.to_string(), "./".to_string())],
            params: vec![],
            body: None,
        };
        let rsp = <super::SpinHttp as spin_http::SpinHttp>::handle_http_request(req);
        assert_eq!(rsp.status, 200);

        // Test against empty path
        let req = spin_http::Request {
            method: spin_http::Method::Get,
            uri: "http://thisistest.com".to_string(),
            headers: vec![(PATH_INFO_HEADER.to_string(), "".to_string())],
            params: vec![],
            body: None,
        };
        let rsp = <super::SpinHttp as spin_http::SpinHttp>::handle_http_request(req);
        assert_eq!(rsp.status, 200);
    }
}
