//! What can go wrong, as one type. `kind()` is what the interface is told.

use std::fmt;

use crate::contract::{EngineError, ErrorKind, PhotoId};

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    UnknownPhoto(PhotoId),
    BadView(String),
    /// A tool id this engine does not have.
    UnknownTool(String),
    UnknownSetting {
        tool: String,
        key: String,
    },
    /// A value of the wrong type, or not one of its choices.
    BadValue {
        tool: String,
        message: String,
    },
    /// An edit this engine cannot read.
    BadEdit(String),
    /// The file could not be read, or rawler could not decode it.
    Decode(String),
    /// A GPU tool ran where no adapter or device could be opened.
    NoGpu(String),
    /// A tool failed while it ran, or was handed the wrong kind of image.
    Tool {
        tool: &'static str,
        message: String,
    },
    /// The registry is inconsistent: an id twice, stages out of order, or
    /// settings the contract cannot describe. A fault in the code.
    Registry(String),
    /// A render gave up because a newer one was asked for. Never reported.
    Cancelled,
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::UnknownPhoto(_) => ErrorKind::UnknownPhoto,
            Error::BadView(_) => ErrorKind::BadView,
            Error::UnknownTool(_) => ErrorKind::UnknownTool,
            Error::UnknownSetting { .. } => ErrorKind::UnknownSetting,
            Error::BadValue { .. } => ErrorKind::BadValue,
            Error::BadEdit(_) => ErrorKind::BadEdit,
            Error::Decode(_) => ErrorKind::Decode,
            Error::NoGpu(_) => ErrorKind::NoGpu,
            Error::Tool { .. } => ErrorKind::Render,
            Error::Registry(_) | Error::Cancelled => ErrorKind::Internal,
        }
    }

    /// As the interface receives it.
    pub fn to_contract(&self) -> EngineError {
        EngineError {
            kind: self.kind(),
            message: self.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownPhoto(id) => write!(f, "no open photo {}", id.0),
            Error::BadView(m) => write!(f, "not a view: {m}"),
            Error::UnknownTool(id) => write!(f, "no tool {id}"),
            Error::UnknownSetting { tool, key } => write!(f, "{tool} has no setting {key}"),
            Error::BadValue { tool, message } => write!(f, "{tool}: {message}"),
            Error::BadEdit(m) => write!(f, "edit: {m}"),
            Error::Decode(m) => write!(f, "decode: {m}"),
            Error::NoGpu(m) => write!(f, "no GPU: {m}"),
            Error::Tool { tool, message } => write!(f, "{tool}: {message}"),
            Error::Registry(m) => write!(f, "registry: {m}"),
            Error::Cancelled => write!(f, "cancelled by a newer render"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
