use std::io::ErrorKind;

#[derive(thiserror::Error, Debug)]
pub enum ConnectionError {
    #[error("Empty connection")]
    NoData,
    #[error("Bad data: {0}")]
    BadData(String),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Could not assign connection to replay")]
    CannotAssignToReplay,
}

// Some helpers.
impl ConnectionError {
    pub fn is_eof(&self) -> bool {
        if let Self::IO(e) = self {
            return e.kind() == ErrorKind::UnexpectedEof;
        }
        return false;
    }

    pub fn is_no_data(&self) -> bool {
        matches!(self, ConnectionError::NoData)
    }
}

// Little shortcut for less typing,
pub fn bad_data(what: impl Into<String>) -> ConnectionError {
    ConnectionError::BadData(what.into())
}

pub type ConnResult<T> = Result<T, ConnectionError>;

// Below code lets us handle errors we don't need the type of.
pub struct SomeError {}

impl<T: std::error::Error> From<T> for SomeError {
    fn from(_: T) -> Self {
        Self {}
    }
}

#[macro_export]
macro_rules! some_error {
    ($e: expr) => {
        (|| -> std::result::Result<_, SomeError> { Ok($e) })()
    };
}

#[derive(thiserror::Error, Debug)]
pub enum SaveError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}
