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

impl ConnectionError {
    pub fn bad_data(what: impl Into<String>) -> Self {
        Self::BadData(what.into())
    }
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

// Below code lets us handle errors we don't need the type of.
pub struct SomeError {
}

impl<T: std::error::Error> From<T> for SomeError {
    fn from(_: T) -> Self {
        Self {}
    }
}

#[macro_export]
macro_rules! some_error {
    ($e: expr) => {
        (|| -> std::result::Result<_, SomeError> {Ok($e) })()
    }
}

#[derive(Error, Debug)]
pub enum SaveError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}
