use std::num::ParseIntError;
use std::str::Utf8Error;

use log::info;
use thiserror::Error;

use crate::accept::header::MaybeConnectionHeader;

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("Empty connection")]
    NoData(),
    #[error("Bad data: {0}")]
    BadData(String),
    #[error("IO error: {context}, {source}")]
    IO {
        source: std::io::Error,
        context: String,
    },
    #[error("Shutting down")]
    ShuttingDown(),
}

// Usually we want to throw BadData.
impl From<String> for ConnectionError {
    fn from(s : String) -> Self {
        Self::BadData(s)
    }
}

impl From<&str> for ConnectionError {
    fn from(s : &str) -> Self {
        Self::BadData(String::from(s))
    }
}

impl From<std::io::Error> for ConnectionError {
    fn from(source: std::io::Error) -> ConnectionError {
        ConnectionError::IO {source, context: String::new()}
    }
}

impl ConnectionError {
    pub fn log(&self, c: MaybeConnectionHeader) {
        match self {
            Self::BadData(..) | Self::IO {..} => info!("{} ended: {}", c, self),
            _ => (),
        }
    }
    pub fn context(self, s: String) -> Self {
        match self {
            Self::BadData(_) => Self::BadData(s),
            Self::IO {source, ..} => Self::IO {source, context: s},
            other => other,
        }
    }
}

pub type ConnResult<T> = Result<T, ConnectionError>;

// Below is some magic for nicer handling of errors.

// Add string context to errors.
pub trait AddContext<T> {
    fn context(self, c: T) -> Self;
}

impl<T> AddContext<String> for ConnResult<T> {
    fn context(self, s: String) -> Self {
        match self {
            Err(e) => Err(e.context(s)),
            ok => ok,
        }
    }
}

impl<T> AddContext<&str> for ConnResult<T> {
    fn context(self, s: &str) -> Self {
        self.context(String::from(s))
    }
}

// Convert all errors in expr to ConnectionError.
#[macro_export]
macro_rules! with_context {
    ($e: expr, $s: expr) => {
        (|| Ok($e))().context($s)
    }
}

// Typical conversions.
impl From<Utf8Error> for ConnectionError {
    fn from(_ :Utf8Error) -> Self {
        "UTF decode error".into()
    }
}

impl From<ParseIntError> for ConnectionError {
    fn from(_ :ParseIntError) -> Self {
        "Int parse error".into()
    }
}
