use std::io::{Read, Seek};

use http::{header::AsHeaderName, HeaderMap};

/// A value that supports both Read and Seek
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

// Extension methods

pub trait HeaderStrings {
    /// Gets the single value of a header as a string. If the header has multiple values,
    /// it returns the first one.
    fn get_str(&self, key: impl AsHeaderName) -> Option<&str>;
}

impl HeaderStrings for HeaderMap {
    fn get_str(&self, key: impl AsHeaderName) -> Option<&str> {
        self.get(key).and_then(|v| v.to_str().ok())
    }
}
