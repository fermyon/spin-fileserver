use anyhow::{anyhow, Context, Result};
use futures::SinkExt;
use http::{
    header::{ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE, ETAG, IF_NONE_MATCH},
    HeaderName, StatusCode, Uri,
};
use spin_sdk::http::{Fields, IncomingRequest, OutgoingResponse, ResponseOutparam};
use std::{
    cmp::Ordering,
    fmt,
    fmt::Error,
    fs::File,
    io::{Cursor, Read},
    path::PathBuf,
    str,
    str::FromStr,
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
/// Gzip content encoding identifier
const GZIP_ENCODING: &str = "gzip";
/// Deflate content encoding identifier
const DEFLATE_ENCODING: &str = "deflate";
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
const DEFLATE_LEVEL: flate2::Compression = flate2::Compression::fast();

#[derive(PartialEq)]
struct ContentEncoding {
    // We limit expressed encodings to ones that we support
    encoding: SupportedEncoding,
    weight: Option<f32>,
}

impl fmt::Display for ContentEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encoding)?;

        if let Some(weight) = self.weight {
            write!(f, ";q={weight}")?;
        }

        Ok(())
    }
}

impl PartialEq<SupportedEncoding> for ContentEncoding {
    fn eq(&self, other: &SupportedEncoding) -> bool {
        self.encoding == *other
    }
}

impl PartialEq<SupportedEncoding> for &ContentEncoding {
    fn eq(&self, other: &SupportedEncoding) -> bool {
        self.encoding == *other
    }
}

impl PartialOrd for ContentEncoding {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let aweight = self.weight.unwrap_or(1.0);
        let bweight = other.weight.unwrap_or(1.0);
        match aweight.partial_cmp(&bweight) {
            Some(Ordering::Equal) => match (self.encoding, other.encoding) {
                // Always prefer brotli
                (SupportedEncoding::Brotli, _) => Some(Ordering::Greater),
                (_, SupportedEncoding::Brotli) => Some(Ordering::Less),
                // Otherwise prefer the more specific option
                (SupportedEncoding::None, _) => Some(Ordering::Less),
                (_, SupportedEncoding::None) => Some(Ordering::Greater),
                // Everything else is roughly equal
                (_, _) => Some(Ordering::Equal),
            },
            v => v,
        }
    }
}

impl FromStr for ContentEncoding {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split(';');
        let encoding = parts.next().unwrap().trim();
        let encoding =
            SupportedEncoding::from_str(encoding).context("failed to parse encoding type")?;
        let Some(weight) = parts
            .next()
            .map(|s| s.trim())
            .and_then(|s| s.strip_prefix("q="))
        else {
            return Ok(ContentEncoding {
                encoding,
                weight: None,
            });
        };

        let mut weight: f32 = weight
            .trim()
            .parse()
            .context("failed to parse encoding weight")?;
        weight = weight.clamp(0.0, 1.0);

        Ok(ContentEncoding {
            encoding,
            weight: Some(weight),
        })
    }
}

/// Common Content Encodings
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum SupportedEncoding {
    Brotli,
    Deflate,
    Gzip,
    None,
}

impl fmt::Display for SupportedEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_content = match self {
            Self::Brotli => BROTLI_ENCODING,
            Self::Deflate => DEFLATE_ENCODING,
            Self::Gzip => GZIP_ENCODING,
            Self::None => "<none>",
        };

        write!(f, "{}", header_content)?;
        Ok(())
    }
}

impl FromStr for SupportedEncoding {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            BROTLI_ENCODING => Ok(Self::Brotli),
            DEFLATE_ENCODING => Ok(Self::Deflate),
            GZIP_ENCODING => Ok(Self::Gzip),
            _ => Ok(Self::None),
        }
    }
}

impl SupportedEncoding {
    /// Return the best SupportedEncoding
    fn best_encoding(headers: &[(String, Vec<u8>)]) -> Self {
        let mut accepted_encodings: Vec<ContentEncoding> = headers
            .iter()
            .filter(|(k, _)| HeaderName::from_bytes(k.as_bytes()).ok() == Some(ACCEPT_ENCODING))
            .flat_map(|(_, v)| {
                str::from_utf8(v).ok().map(|v| {
                    v.split(',')
                        .map(|v| ContentEncoding::from_str(v).ok())
                        .filter(|v| match v {
                            Some(y) => match y.encoding {
                                // Filter out "None" values to ensure some compression is
                                // preferred. This is mostly to be defensive to types we don't
                                // understand as we only parse encodings we support.
                                // It's probably subpar if somebody actually _doesn't_ want
                                // compression but supports it anyway.
                                SupportedEncoding::None => false,
                                _ => true,
                            },
                            None => false,
                        })
                        .flatten()
                })
            })
            .flatten()
            .collect();

        accepted_encodings.sort_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));

        accepted_encodings
            .first()
            .map(|v| v.encoding)
            .unwrap_or(SupportedEncoding::None)
    }
}

#[spin_sdk::http_component]
async fn handle_request(req: IncomingRequest, res_out: ResponseOutparam) {
    let headers = req.headers().entries();
    let enc = SupportedEncoding::best_encoding(&headers);
    let mut path = headers
        .iter()
        .find_map(|(k, v)| (k.to_lowercase() == PATH_INFO_HEADER).then_some(v))
        .expect("PATH_INFO header must be set by the Spin runtime");

    let component_route = headers
        .iter()
        .find_map(|(k, v)| (k.to_lowercase() == COMPONENT_ROUTE_HEADER).then_some(v))
        .expect("COMPONENT_ROUTE header must be set by the Spin runtime");

    let uri = req
        .uri()
        .parse::<Uri>()
        .expect("URI is invalid")
        .path()
        .as_bytes()
        .to_vec();
    if &uri == component_route && path.is_empty() {
        path = &uri;
    }

    let if_none_match = headers
        .iter()
        .find_map(|(k, v)| {
            (HeaderName::from_bytes(k.as_bytes()).ok()? == IF_NONE_MATCH).then_some(v.as_slice())
        })
        .unwrap_or(b"");
    match FileServer::make_response(path, enc, if_none_match) {
        Ok((status, headers, reader)) => {
            let res = OutgoingResponse::new(status.into(), &Fields::new(&headers));
            let mut body = res.take_body();
            res_out.set(res);
            if let Some(mut reader) = reader {
                let mut buffer = vec![0_u8; BUFFER_SIZE];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => {
                            if let Err(e) = body.send(buffer[..count].to_vec()).await {
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
        }
        Err(e) => {
            eprintln!("Error building response: {e}");
            let res = OutgoingResponse::new(500, &Fields::new(&[]));
            let mut body = res.take_body();
            res_out.set(res);
            if let Err(e) = body.send(b"Internal Server Error".to_vec()).await {
                eprintln!("Error sending body: {e}");
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
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
    fn resolve_and_read(path: &str, encoding: SupportedEncoding) -> Option<Result<Box<dyn Read>>> {
        let reader = match Self::resolve(path) {
            FileServerPath::Physical(path) => {
                Some(Self::read(&path).map(|r| Box::new(r) as Box<dyn Read>))
            }
            FileServerPath::Embedded(resource) => {
                Some(Ok(Box::new(Cursor::new(resource)) as Box<dyn Read>))
            }
            FileServerPath::None => None,
        }?;

        Some(reader.map(|reader| match encoding {
            SupportedEncoding::Brotli => Box::new(brotli::CompressorReader::new(
                reader,
                BUFFER_SIZE,
                BROTLI_LEVEL,
                20,
            )) as Box<dyn Read>,
            SupportedEncoding::Deflate => {
                Box::new(flate2::read::DeflateEncoder::new(reader, DEFLATE_LEVEL))
            }
            SupportedEncoding::Gzip => {
                Box::new(flate2::read::GzEncoder::new(reader, DEFLATE_LEVEL))
            }
            SupportedEncoding::None => reader,
        }))
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

    /// Open the file given its path and return its content and content type header.
    fn read(path: &PathBuf) -> Result<impl Read> {
        File::open(path).with_context(|| anyhow!("cannot open {}", path.display()))
    }

    /// Return the media type of the file based on the path.
    fn mime(path: &str) -> Option<String> {
        match path {
            FAVICON_ICO_FILENAME => mime_guess::from_ext("ico"),
            FAVICON_PNG_FILENAME => mime_guess::from_ext("png"),
            _ => mime_guess::from_path(path),
        }
        .first()
        .map(|m| m.to_string())
    }

    fn make_headers(path: &str, enc: SupportedEncoding, etag: &str) -> Vec<(String, Vec<u8>)> {
        let mut headers = Vec::new();
        let cache_control = match std::env::var(CACHE_CONTROL_ENV) {
            Ok(c) => c,
            Err(_) => CACHE_CONTROL_DEFAULT_VALUE.to_string(),
        };
        headers.push((
            CACHE_CONTROL.as_str().to_string(),
            cache_control.into_bytes(),
        ));
        headers.push((ETAG.as_str().to_string(), etag.as_bytes().to_vec()));

        match enc {
            SupportedEncoding::Brotli => headers.push((
                CONTENT_ENCODING.as_str().to_string(),
                BROTLI_ENCODING.as_bytes().to_vec(),
            )),
            SupportedEncoding::Deflate => headers.push((
                CONTENT_ENCODING.as_str().to_string(),
                DEFLATE_ENCODING.as_bytes().to_vec(),
            )),
            SupportedEncoding::Gzip => headers.push((
                CONTENT_ENCODING.as_str().to_string(),
                GZIP_ENCODING.as_bytes().to_vec(),
            )),
            SupportedEncoding::None => {}
        }

        if let Some(mime) = Self::mime(path) {
            headers.push((CONTENT_TYPE.as_str().to_string(), mime.into_bytes()));
        };

        headers
    }

    #[allow(clippy::type_complexity)]
    fn make_response(
        path: &[u8],
        enc: SupportedEncoding,
        if_none_match: &[u8],
    ) -> Result<(StatusCode, Vec<(String, Vec<u8>)>, Option<Box<dyn Read>>)> {
        let path = str::from_utf8(path)?;
        let reader = Self::resolve_and_read(path, enc).transpose()?;
        let etag = Self::make_etag(reader)?;
        let mut reader = Self::resolve_and_read(path, enc).transpose()?;
        let mut headers = Self::make_headers(path, enc, &etag);

        let status = if reader.is_some() {
            if etag.as_bytes() == if_none_match {
                reader = None;
                StatusCode::NOT_MODIFIED
            } else {
                StatusCode::OK
            }
        } else {
            reader = Some(Box::new(Cursor::new(b"Not Found")));
            headers = Vec::new();
            StatusCode::NOT_FOUND
        };

        Ok((status, headers, reader))
    }

    fn make_etag(body: Option<Box<dyn Read>>) -> Result<String> {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        if let Some(mut reader) = body {
            let mut buffer = vec![0_u8; BUFFER_SIZE];
            loop {
                match reader.read(&mut buffer)? {
                    0 => break,
                    count => {
                        hasher.update(&buffer[..count]);
                    }
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

    #[test]
    fn test_best_encoding_none() {
        let enc = SupportedEncoding::best_encoding(&[]);
        assert_eq!(enc, SupportedEncoding::None);
    }

    #[test]
    fn test_best_encoding_with_unknown() {
        let enc = SupportedEncoding::best_encoding(&[(
            ACCEPT_ENCODING.to_string(),
            b"some-weird-encoding".to_vec(),
        )]);
        assert_eq!(enc, SupportedEncoding::None);
    }

    #[test]
    fn test_best_encoding_with_weights() {
        let enc = SupportedEncoding::best_encoding(&[(
            ACCEPT_ENCODING.to_string(),
            b"gzip;br;q=0.1".to_vec(),
        )]);
        assert_eq!(enc, SupportedEncoding::Gzip);
    }

    #[test]
    fn test_best_encoding_with_multiple_headers() {
        let enc = SupportedEncoding::best_encoding(&[
            (ACCEPT_ENCODING.to_string(), b"gzip".to_vec()),
            (ACCEPT_ENCODING.to_string(), b"br".to_vec()),
        ]);
        assert_eq!(enc, SupportedEncoding::Brotli);
    }

    #[test]
    fn test_best_encoding_with_gzip() {
        let enc =
            SupportedEncoding::best_encoding(&[(ACCEPT_ENCODING.to_string(), b"gzip".to_vec())]);
        assert_eq!(enc, SupportedEncoding::Gzip);
    }

    #[test]
    fn test_best_encoding_with_deflate() {
        let enc =
            SupportedEncoding::best_encoding(&[(ACCEPT_ENCODING.to_string(), b"deflate".to_vec())]);
        assert_eq!(enc, SupportedEncoding::Deflate);
    }

    #[test]
    fn test_best_encoding_with_br() {
        let enc =
            SupportedEncoding::best_encoding(&[(ACCEPT_ENCODING.to_string(), b"gzip,br".to_vec())]);
        assert_eq!(enc, SupportedEncoding::Brotli);
    }

    #[test]
    fn test_serve_file_found() {
        let (status, ..) =
            FileServer::make_response(b"./hello-test.txt", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);
    }

    #[test]
    fn test_serve_with_etag() {
        let (status, _, reader) = FileServer::make_response(
            b"./hello-test.txt",
            SupportedEncoding::None,
            b"4dca0fd5f424a31b03ab807cbae77eb32bf2d089eed1cee154b3afed458de0dc",
        )
        .unwrap();
        assert_eq!(status, StatusCode::NOT_MODIFIED);
        assert!(reader.is_none());
    }

    #[test]
    fn test_serve_file_not_found() {
        let (status, _, reader) =
            FileServer::make_response(b"non-exisitent-file", SupportedEncoding::None, b"").unwrap();
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
            FileServer::make_response(b"non-exisitent-file", SupportedEncoding::None, b"").unwrap();
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
            FileServer::make_response(b"non-exisitent-file", SupportedEncoding::None, b"").unwrap();
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
            FileServer::make_response(b"non-exisitent-file", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);
        let mut actual_body = Vec::new();
        reader.unwrap().read_to_end(&mut actual_body).unwrap();
        assert_eq!(actual_body, expected_body);
    }

    #[test]
    fn test_serve_index() {
        // Test against path with trailing slash
        let (status, ..) = FileServer::make_response(b"./", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);

        // Test against empty path
        let (status, ..) = FileServer::make_response(b"", SupportedEncoding::None, b"").unwrap();
        assert_eq!(status, StatusCode::OK);
    }

    #[test]
    fn test_serve_fallback_favicon() {
        let (status, _, reader) = FileServer::make_response(
            FAVICON_PNG_FILENAME.as_bytes(),
            SupportedEncoding::None,
            b"",
        )
        .unwrap();
        assert_eq!(status, StatusCode::OK);
        let mut actual_body = Vec::new();
        reader.unwrap().read_to_end(&mut actual_body).unwrap();
        assert_eq!(actual_body, FALLBACK_FAVICON_PNG);
    }
}
