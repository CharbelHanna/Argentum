//! What can go wrong, as one type.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The file could not be read, or rawler could not decode it.
    Decode(String),
    /// A GPU tool ran where no adapter or device could be opened.
    NoGpu(String),
    /// A tool's settings could not be read: malformed, written by a newer
    /// engine, or from a version with no migration.
    Settings { tool: String, message: String },
    /// The registry is inconsistent: an id twice, or stages out of order.
    Registry(String),
    /// A tool failed while it ran, or was handed the wrong kind of image.
    Tool { tool: &'static str, message: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Decode(m) => write!(f, "decode: {m}"),
            Error::NoGpu(m) => write!(f, "no GPU: {m}"),
            Error::Settings { tool, message } => write!(f, "{tool} settings: {message}"),
            Error::Registry(m) => write!(f, "registry: {m}"),
            Error::Tool { tool, message } => write!(f, "{tool}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
