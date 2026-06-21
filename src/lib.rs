mod encoding;
mod traits;

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use http::{
    header::{CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE, ETAG, IF_NONE_MATCH},
    HeaderValue, StatusCode,
};
use spin_sdk::http::{HeaderMap, IntoResponse, Request, Response};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::PathBuf,
    str,
};
use traits::{HeaderStrings, ReadSeek};

use crate::encoding::SupportedEncoding;

/// The default value for the cache control header.
const CACHE_CONTROL_DEFAULT_VALUE: &str = "max-age=60";
/// Environment variable for the cache configuration.
const CACHE_CONTROL_ENV: &str = "CACHE_CONTROL";
/// The path info header.
const PATH_INFO_HEADER: &str = "spin-path-info";
/// The component route header
const COMPONENT_ROUTE_HEADER: &str = "spin-component-route";
// Environment variable for the fallback path
const FALLBACK_PATH_ENV: &str = "FALLBACK_PATH";
/// Environment variable for the custom 404 path
const CUSTOM_404_PATH_ENV: &str = "CUSTOM_404_PATH";
/// Directory fallback path (trying to map `/about/` -> `/about/index.html`).
const DIRECTORY_FALLBACK_PATH: &str = "index.html";
// FAVICON_ICO_FILENAME
const FAVICON_ICO_FILENAME: &str = "favicon.ico";
// FAVICON_PNG_FILENAME
const FAVICON_PNG_FILENAME: &str = "favicon.png";
// Fallback favicon.png that is used when user does not supply a custom one
const FALLBACK_FAVICON_PNG: &[u8] = include_bytes!("../spin-favicon.png");
// Fallback favicon.ico that is used when user does not supply a custom one
const FALLBACK_FAVICON_ICO: &[u8] = include_bytes!("../spin-favicon.ico");

const BUFFER_SIZE: usize = 64 * 1024;

#[spin_sdk::http_service]
async fn handle_request(req: Request) -> anyhow::Result<impl IntoResponse> {
    let headers = req.headers();
    let enc = SupportedEncoding::best_encoding(headers);
    let mut path = headers
        .get_str(PATH_INFO_HEADER)
        .expect("PATH_INFO header must be set by the Spin runtime");

    let component_route = headers
        .get_str(COMPONENT_ROUTE_HEADER)
        .expect("COMPONENT_ROUTE header must be set by the Spin runtime");

    let uri = req.uri().path();
    if uri == component_route && path.is_empty() {
        path = uri;
    }

    let if_none_match = headers
        .get(IF_NONE_MATCH)
        .map(|v| v.as_bytes())
        .unwrap_or(b"");

    let (mut tx, rx) = futures::channel::mpsc::channel(16);
    let rx = rx.map(move |value| anyhow::Ok(http_body::Frame::data(Bytes::from_owner(value))));
    let body = http_body_util::StreamBody::new(rx);
    let mut res = Response::new(body);

    match FileServer::make_response(path, enc, if_none_match) {
        Ok((status, headers, reader)) => {
            *res.status_mut() = status;
            *res.headers_mut() = headers;

            spin_sdk::wasip3::spawn(async move {
                if let Some(mut reader) = reader {
                    loop {
                        let mut buffer = vec![0_u8; BUFFER_SIZE];
                        match reader.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(count) => {
                                buffer.truncate(count);
                                if let Err(e) = tx.send(buffer).await {
                                    eprintln!("Error sending body: {e}");
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("Error reading file: {e}");
                                break;
                            }
                        }
                    }
                }
            });

            Ok(res)
        }
        Err(e) => {
            eprintln!("Error building response: {e}");
            if let Err(e) = tx.send(b"Internal Server Error".to_vec()).await {
                eprintln!("Error sending body: {e}");
                anyhow::bail!("Internal server error");
            }
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            Ok(res)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FileServerPath {
    Physical(PathBuf),
    Embedded(&'static [u8]),
    None,
}

trait IsFavicon {
    fn is_favicon(&self) -> bool;
}

impl IsFavicon for PathBuf {
    fn is_favicon(&self) -> bool {
        match self.clone().file_name() {
            Some(s) => s == FAVICON_ICO_FILENAME || s == FAVICON_PNG_FILENAME,
            None => false,
        }
    }
}

struct FileServer;
impl FileServer {
    /// Resolve the requested path and then try to read the file.
    /// None should indicate that the file does not exist after attempting fallback paths.
    fn read(path: FileServerPath) -> Option<Result<Box<dyn ReadSeek>>> {
        match path {
            FileServerPath::Physical(path) => {
                Some(Self::read_file(&path).map(|r| Box::new(r) as Box<dyn ReadSeek>))
            }
            FileServerPath::Embedded(resource) => {
                Some(Ok(Box::new(Cursor::new(resource)) as Box<dyn ReadSeek>))
            }
            FileServerPath::None => None,
        }
    }

    /// Resolve the request path to a file path.
    /// Returns a `FileServerPath` variant.
    fn resolve(req_path: &str) -> FileServerPath {
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

        // if path doesn't exist and a favicon is requested, return with corresponding embedded resource
        if !path.exists() && path.is_favicon() {
            return match path.extension() {
                Some(os_string) => match os_string.to_str() {
                    Some("ico") => FileServerPath::Embedded(FALLBACK_FAVICON_ICO),
                    Some("png") => FileServerPath::Embedded(FALLBACK_FAVICON_PNG),
                    _ => FileServerPath::None,
                },
                None => FileServerPath::None,
            };
        }
        // if still haven't found a file, override with the user-configured fallback path
        if !path.exists() {
            if let Ok(fallback_path) = std::env::var(FALLBACK_PATH_ENV) {
                path = PathBuf::from(fallback_path);
            }
        }

        if path.exists() {
            return FileServerPath::Physical(path);
        }

        // check if user configured a custom 404 path
        // if so, check if that path exists and return it instead of sending a plain 404
        if let Ok(custom_404) = std::env::var(CUSTOM_404_PATH_ENV) {
            path = PathBuf::from(custom_404);
        }

        if path.exists() {
            FileServerPath::Physical(path)
        } else {
            FileServerPath::None
        }
    }

    /// Open the file given its path and return its content.
    fn read_file(path: &PathBuf) -> Result<impl ReadSeek> {
        File::open(path).with_context(|| anyhow!("cannot open {}", path.display()))
    }

    /// Return the media type of the file based on the path.
    fn mime(path: &str) -> Option<String> {
        let mut mime = match path {
            FAVICON_ICO_FILENAME => mime_guess::from_ext("ico"),
            FAVICON_PNG_FILENAME => mime_guess::from_ext("png"),
            _ => mime_guess::from_path(path),
        }
        .first();

        if mime.is_none() {
            if let FileServerPath::Physical(p) = Self::resolve(path) {
                mime = mime_guess::from_path(&p).first();
            }
        }

        mime.map(|m| m.to_string())
    }

    fn make_headers(path: &str, enc: SupportedEncoding, etag: &str) -> anyhow::Result<HeaderMap> {
        let mut headers = HeaderMap::new();

        let cache_control = match std::env::var(CACHE_CONTROL_ENV) {
            Ok(c) => c,
            Err(_) => CACHE_CONTROL_DEFAULT_VALUE.to_string(),
        };
        headers.append(
            CACHE_CONTROL,
            HeaderValue::from_str(&cache_control).context("invalid CACHE_CONTROL env")?,
        );
        headers.append(ETAG, HeaderValue::from_str(etag).context("invalid etag")?);

        let encoding_header = enc.header_value();

        if let Some(encoding_header) = encoding_header {
            headers.append(
                CONTENT_ENCODING,
                HeaderValue::from_str(encoding_header).context("invalid encoding")?,
            );
        }

        if let Some(mime) = Self::mime(path) {
            headers.append(
                CONTENT_TYPE,
                HeaderValue::from_str(&mime).context("invalid mime type")?,
            );
        };

        Ok(headers)
    }

    #[allow(clippy::type_complexity)]
    fn make_response(
        path: &str,
        enc: SupportedEncoding,
        if_none_match: &[u8],
    ) -> Result<(StatusCode, HeaderMap, Option<Box<dyn Read>>)> {
        let resolved_path = Self::resolve(path);
        let reader = Self::read(resolved_path.clone()).transpose()?;

        let Some(mut reader) = reader else {
            return Self::not_found();
        };

        let etag = Self::make_etag(&mut reader)?;
        let headers = Self::make_headers(path, enc, &etag)?;
        if etag.as_bytes() == if_none_match {
            return Ok((StatusCode::NOT_MODIFIED, headers, None));
        }

        reader.seek(std::io::SeekFrom::Start(0))?;

        let reader = enc.encode(reader);
        Ok((StatusCode::OK, headers, Some(reader)))
    }

    #[allow(clippy::type_complexity)]
    fn not_found() -> Result<(StatusCode, HeaderMap, Option<Box<dyn Read>>)> {
        let body = Box::new(Cursor::new(b"Not Found"));
        Ok((StatusCode::NOT_FOUND, Default::default(), Some(body)))
    }

    fn make_etag(body: &mut dyn Read) -> Result<String> {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        let mut buffer = vec![0_u8; BUFFER_SIZE];

        loop {
            match body.read(&mut buffer)? {
                0 => break,
                count => {
                    hasher.update(&buffer[..count]);
                }
            }
        }

        Ok(hex::encode(hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header::ACCEPT_ENCODING;
    use scopeguard::defer;
    use std::{fs, path::Path, sync::Mutex};

    static TEST_ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn header_map(headers: &[(http::HeaderName, &str)]) -> HeaderMap {
        let mut header_map = HeaderMap::new();

        for (header_name, header_text) in headers {
            header_map.append(
                header_name,
                HeaderValue::from_str(*header_text).expect("bad header text"),
            );
        }

        header_map
    }

    #[test]
    fn test_best_encoding_none() {
        let enc = SupportedEncoding::best_encoding(&&header_map(&[]));
        assert_eq!(enc, SupportedEncoding::None);
    }

    #[test]
    fn test_best_encoding_with_unknown() {
        let enc = SupportedEncoding::best_encoding(&header_map(&[(
            ACCEPT_ENCODING,
            "some-weird-encoding",
        )]));
        assert_eq!(enc, SupportedEncoding::None);
    }

    #[test]
    fn test_best_encoding_with_weights() {
        let enc = SupportedEncoding::best_encoding(&header_map(&[
            (ACCEPT_ENCODING, "gzip"),
            (ACCEPT_ENCODING, "br;q=0.1"),
        ]));
        assert_eq!(enc, SupportedEncoding::Gzip);

        let enc = SupportedEncoding::best_encoding(&header_map(&[
            (ACCEPT_ENCODING, "gzip;q=0.1"),
            (ACCEPT_ENCODING, "br;q=0.9"),
        ]));
        assert_eq!(enc, SupportedEncoding::Brotli);
    }

    #[test]
    fn test_best_encoding_with_multiple_headers() {
        let enc = SupportedEncoding::best_encoding(&header_map(&[
            (ACCEPT_ENCODING, "gzip"),
            (ACCEPT_ENCODING, "br"),
        ]));
        assert_eq!(enc, SupportedEncoding::Brotli);
    }

    #[test]
    fn test_best_encoding_with_gzip() {
        let enc = SupportedEncoding::best_encoding(&header_map(&[(ACCEPT_ENCODING, "gzip")]));
        assert_eq!(enc, SupportedEncoding::Gzip);
    }

    #[test]
    fn test_best_encoding_with_deflate() {
        let enc = SupportedEncoding::best_encoding(&header_map(&[(ACCEPT_ENCODING, "deflate")]));
        assert_eq!(enc, SupportedEncoding::Deflate);
    }

    #[test]
    fn test_best_encoding_with_br() {
        let enc = SupportedEncoding::best_encoding(&header_map(&[
            (ACCEPT_ENCODING, "gzip"),
            (ACCEPT_ENCODING, "br"),
        ]));
        assert_eq!(enc, SupportedEncoding::Brotli);
    }

    #[test]
    fn test_serve_file_found() {
        let (status, ..) =
            FileServer::make_response("./hello-test.txt", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);
    }

    #[test]
    fn test_serve_with_etag() {
        let (status, _, reader) = FileServer::make_response(
            "./hello-test.txt",
            SupportedEncoding::None,
            b"4dca0fd5f424a31b03ab807cbae77eb32bf2d089eed1cee154b3afed458de0dc",
        )
        .unwrap();
        assert_eq!(status, StatusCode::NOT_MODIFIED);
        assert!(reader.is_none());
    }

    #[test]
    fn test_serve_file_not_found() {
        // Although this doesn't modify the environment, it can be affected if
        // another test is concurrently meddling with the environment.
        let _lock = TEST_ENV_MUTEX.lock().unwrap();

        let (status, _, reader) =
            FileServer::make_response("non-exisitent-file", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::NOT_FOUND);
        let mut actual_body = Vec::new();
        reader.unwrap().read_to_end(&mut actual_body).unwrap();
        assert_eq!(actual_body.as_slice(), b"Not Found");
    }

    #[test]
    fn test_serve_custom_404() {
        let _lock = TEST_ENV_MUTEX.lock().unwrap();

        // reuse existing asset as custom 404 doc
        let custom_404_path = "hello-test.txt";
        let expected_body =
            fs::read(Path::new(custom_404_path)).expect("Could not read custom 404 file");

        std::env::set_var(CUSTOM_404_PATH_ENV, custom_404_path);
        defer! {
            std::env::remove_var(CUSTOM_404_PATH_ENV);
        }

        let (status, _, reader) =
            FileServer::make_response("non-exisitent-file", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);
        let mut actual_body = Vec::new();
        reader.unwrap().read_to_end(&mut actual_body).unwrap();
        assert_eq!(actual_body, expected_body);
    }

    #[test]
    fn test_serve_non_existing_custom_404() {
        let _lock = TEST_ENV_MUTEX.lock().unwrap();

        // provide a invalid path
        let custom_404_path = "non-existing-404.html";

        std::env::set_var(CUSTOM_404_PATH_ENV, custom_404_path);
        defer! {
            std::env::remove_var(CUSTOM_404_PATH_ENV);
        }

        let (status, _, reader) =
            FileServer::make_response("non-exisitent-file", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::NOT_FOUND);
        let mut actual_body = Vec::new();
        reader.unwrap().read_to_end(&mut actual_body).unwrap();
        assert_eq!(actual_body.as_slice(), b"Not Found");
    }

    #[test]
    fn test_serve_file_not_found_with_fallback_path() {
        let _lock = TEST_ENV_MUTEX.lock().unwrap();

        // reuse existing asset as fallback
        let fallback_path = "hello-test.txt";
        let expected_body =
            fs::read(Path::new(fallback_path)).expect("Could not read fallback file");

        std::env::set_var(FALLBACK_PATH_ENV, fallback_path);
        defer! {
            std::env::remove_var(FALLBACK_PATH_ENV);
        }

        let (status, _, reader) =
            FileServer::make_response("non-exisitent-file", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);
        let mut actual_body = Vec::new();
        reader.unwrap().read_to_end(&mut actual_body).unwrap();
        assert_eq!(actual_body, expected_body);
    }

    #[test]
    fn test_serve_index() {
        // Test against path with trailing slash
        let (status, ..) = FileServer::make_response("./", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);

        // Test against empty path
        let (status, ..) = FileServer::make_response("", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);
    }

    #[test]
    fn test_serve_fallback_favicon() {
        let (status, _, reader) =
            FileServer::make_response(FAVICON_PNG_FILENAME, SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);
        let mut actual_body = Vec::new();
        reader.unwrap().read_to_end(&mut actual_body).unwrap();
        assert_eq!(actual_body, FALLBACK_FAVICON_PNG);
    }
}
