use crate::Format;
use std::fmt;

/// Errors produced while reading or decoding soundings.
#[derive(Debug)]
pub enum Error {
    /// Underlying file-system error.
    Io(std::io::Error),
    /// Content did not look like any supported format.
    UnknownFormat,
    /// The content was recognised but is malformed.
    Parse { format: Format, line: Option<usize>, message: String },
    /// Decoding succeeded but produced no usable levels.
    NoData { format: Format },
    /// Manifest/JSON problems unrelated to a specific sounding format.
    Json(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn parse(format: Format, line: Option<usize>, message: impl Into<String>) -> Self {
        Error::Parse { format, line, message: message.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::UnknownFormat => f.write_str("unrecognised sounding format"),
            Error::Parse { format, line: Some(l), message } => {
                write!(f, "{format} parse error at line {l}: {message}")
            }
            Error::Parse { format, line: None, message } => write!(f, "{format} parse error: {message}"),
            Error::NoData { format } => write!(f, "{format}: no usable sounding levels"),
            Error::Json(m) => write!(f, "JSON error: {m}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
