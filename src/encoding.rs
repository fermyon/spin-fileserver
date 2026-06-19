use super::traits::HeaderStrings;
use super::BUFFER_SIZE;
use anyhow::{Context, Result};
use http::header::ACCEPT_ENCODING;
use spin_sdk::http::HeaderMap;
use std::{
    cmp::Ordering,
    fmt::{self, Error},
    io::Read,
    str::{self, FromStr},
};

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

const DEFLATE_LEVEL: flate2::Compression = flate2::Compression::fast();

#[derive(PartialEq, Debug)]
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

        write!(f, "{header_content}")?;
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
    pub fn best_encoding(headers: &HeaderMap) -> Self {
        let Some(accept_encoding_header) = headers.get_str(ACCEPT_ENCODING) else {
            return Self::None;
        };

        let header_vals = accept_encoding_header.split(',');

        let mut accepted_encodings: Vec<ContentEncoding> = header_vals
            .filter_map(|v| {
                let e = ContentEncoding::from_str(v).ok()?;
                // Filter out "None" values to ensure some compression is
                // preferred. This is mostly to be defensive to types we don't
                // understand as we only parse encodings we support.
                // It's probably subpar if somebody actually _doesn't_ want
                // compression but supports it anyway.
                (e.encoding != SupportedEncoding::None).then_some(e)
            })
            .collect();

        accepted_encodings.sort_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));

        accepted_encodings
            .first()
            .map(|v| v.encoding)
            .unwrap_or(SupportedEncoding::None)
    }

    pub fn header_value(&self) -> Option<&'static str> {
        match self {
            SupportedEncoding::Brotli => Some(BROTLI_ENCODING),
            SupportedEncoding::Deflate => Some(DEFLATE_ENCODING),
            SupportedEncoding::Gzip => Some(GZIP_ENCODING),
            SupportedEncoding::None => None,
        }
    }

    pub fn encode(&self, reader: Box<dyn Read>) -> Box<dyn Read> {
        match self {
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
        }
    }
}
