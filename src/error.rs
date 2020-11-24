use std::error::Error;
use std::fmt::Display;
use std::str::Utf8Error;

#[derive(Debug)]
pub struct ConnectionError {
}

impl Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TODO")
    }
}

impl Error for ConnectionError {
}

impl From<std::io::Error> for ConnectionError {
    fn from(err: std::io::Error) -> ConnectionError {
        ConnectionError {}  // FIXME
    }
}

impl From<Utf8Error> for ConnectionError {
    fn from(err: Utf8Error) -> ConnectionError {
        ConnectionError {}  // FIXME
    }
}

pub type ConnResult<T> = Result<T, ConnectionError>;

pub trait ToCerr {
    type Return;
    type Err: Into<ConnectionError>;
    fn tocerr(self) -> Result<Self::Return, ConnectionError>;
}

impl<R, E: Into<ConnectionError>> ToCerr for Result<R, E> {
    type Return = R;
    type Err = E;
    fn tocerr(self) -> Result<R, ConnectionError> {
        self.map_err(E::into)
    }
}
