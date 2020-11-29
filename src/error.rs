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
    #[error("IO error: {source}")]
    IO {
        #[from]
        source: std::io::Error,
    },
    #[error("Shutting down")]
    ShuttingDown(),
}

impl ConnectionError {
    pub fn log(&self, c: MaybeConnectionHeader) {
        match self {
            ConnectionError::BadData(..) | ConnectionError::IO {..} => info!("{} ended: {}", c, self),
            _ => (),
        }
    }
}

pub type ConnResult<T> = Result<T, ConnectionError>;

// Below is some magic for nicer handling of errors.

impl From<String> for ConnectionError {
    fn from(s : String) -> Self {
        Self::BadData(s)    // That's the most common error type
    }
}

impl From<&str> for ConnectionError {
    fn from(s : &str) -> Self {
        Self::BadData(String::from(s))
    }
}

// Add string context to errors.
pub trait AddContext<T> {
    fn context(self, c: T) -> Self;
}

impl<T> AddContext<String> for ConnResult<T> {
    fn context(self, s: String) -> Self {
        match self {
            Err(ConnectionError::BadData(_)) => Err(ConnectionError::BadData(s)),
            e => e,
        }
    }
}

impl<T> AddContext<&str> for ConnResult<T> {
    fn context(self, s: &str) -> Self {
        self.context(String::from(s))
    }
}

// A hacky substitute for try-blocks. Don't use in hot code.
#[macro_export]
macro_rules! as_conn_err {
    ( $t: ty, $e: expr, $s: expr) => {
        || -> ConnResult<$t> {
            Ok($e)
        }().context($s)?
    }
}

// Shorthand for returning an 'error' if we're shutting down. Safer than implementing conversion
// from None, since this way we'll never convert accidentally.
#[macro_export]
macro_rules! or_conn_shutdown {
    ($e: expr, $token: expr) => {
        match $token.stop_future($e).await {
            Some(e) => e,
            None => Err(ConnectionError::ShuttingDown())
        }
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
