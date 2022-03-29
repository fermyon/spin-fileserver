use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use http::{
    header::{ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE},
    HeaderMap, StatusCode,
};
use spin_sdk::http::{not_found, Request, Response};
use std::{fs::File, io::Read};

/// The default value for the cache control header.
const CACHE_CONTROL_DEFAULT_VALUE: &str = "max-age=31536000";
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

/// Common Content Encodings
#[derive(PartialEq)]
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

    let (body, status) = match FileServer::read(path, &enc) {
        Ok(b) => (Some(b), StatusCode::OK),
        Err(e) => {
            eprintln!("Cannot read file: {:?}", e);
            return not_found();
        }
    };

    FileServer::send(status, body, path, enc)
}

struct FileServer;
impl FileServer {
    /// Open the file given its path and return its content and content type header.
    fn read(path: &str, encoding: &ContentEncoding) -> Result<Bytes> {
        let mut file = File::open(path).with_context(|| anyhow!("cannot open {}", path))?;
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

    fn append_headers(path: &str, enc: ContentEncoding, headers: &mut HeaderMap) -> Result<()> {
        let cache_control = match std::env::var(CACHE_CONTROL_ENV) {
            Ok(c) => c.try_into()?,
            Err(_) => CACHE_CONTROL_DEFAULT_VALUE.try_into()?,
        };
        headers.insert(CACHE_CONTROL, cache_control);

        if enc == ContentEncoding::Brotli {
            headers.insert(CONTENT_ENCODING, BROTLI_ENCODING.try_into()?);
        }

        if let Some(m) = Self::mime(path) {
            headers.insert(CONTENT_TYPE, m.try_into()?);
        };

        Ok(())
    }

    fn send(
        status: StatusCode,
        body: Option<Bytes>,
        path: &str,
        enc: ContentEncoding,
    ) -> Result<Response> {
        let mut res = http::Response::builder().status(status);
        let headers = res
            .headers_mut()
            .ok_or(anyhow!("cannot get headers for response"))?;
        FileServer::append_headers(path, enc, headers)?;

        Ok(res.body(body)?)
    }
}
