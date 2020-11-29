use std::fmt::Formatter;
use std::io::ErrorKind;
use std::str::from_utf8;
use stop_token::StopToken;

use crate::as_conn_err;
use crate::error::{ConnResult,AddContext};
use crate::error::ConnectionError;
use crate::or_conn_shutdown;
use crate::server::connection::Connection;

pub enum ConnectionType {
    READER = 1,
    WRITER = 2,
}

pub struct ConnectionHeader {
    pub type_: ConnectionType,
    pub id: u64,
    pub name: String,
}

// Just for logging
pub struct MaybeConnectionHeader {
    v: Option<ConnectionHeader>,
}

impl From<Option<ConnectionHeader>> for MaybeConnectionHeader {
    fn from(v: Option<ConnectionHeader>) -> MaybeConnectionHeader {
        MaybeConnectionHeader {v}
    }
}

impl std::fmt::Display for MaybeConnectionHeader {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match &self.v {
            None => f.write_str("Initial connection"),
            Some(h) => write!(f, "{} '{}' for replay {}", match h.type_ {
                ConnectionType::READER => "Reader",
                ConnectionType::WRITER => "Writer",
            }, h.name, h.id)
        }
    }
}

pub struct ConnectionHeaderReader {
    shutdown_token: StopToken,
}

impl ConnectionHeaderReader {
    pub fn new(shutdown_token: StopToken) -> Self { Self { shutdown_token } }

    async fn read_type(conn: &mut Connection) -> ConnResult<ConnectionType>
    {
        let mut buf: [u8; 2] = [0; 2];
        conn.read_exact(&mut buf).await?;
        match &buf {
            b"P/" => Ok(ConnectionType::WRITER),
            b"G/" => Ok(ConnectionType::READER),
            _ => Err(format!("Invalid connection type: {:x?}", buf).into())
        }
    }

    async fn read_game_data(conn: &mut Connection) -> ConnResult<(u64, String)>
    {
        let mut line: Vec<u8> = Vec::new();
        conn.read_until(b'\0', &mut line, 1024).await?;
        let pieces: &Vec<&[u8]> = &line[..].split(|val| {val == &b'/'}).collect();
        if pieces.len() != 2 {
            return Err(ConnectionError::BadData(format!("Connection header has wrong number of slashes: {}", pieces.len())));
        }

        let id = as_conn_err!(u64, from_utf8(pieces[0])?.parse::<u64>()?,
        "Failed to parse replay ID");
        let name = as_conn_err!(String, String::from(from_utf8(pieces[1])?),
        "Failed to decode connection string id");
        Ok((id, name))
    }

    async fn read_connection_header(conn: &mut Connection) -> ConnResult<ConnectionHeader>
    {
        let type_ = Self::read_type(conn).await.map_err(|e| {
            match e {
                ConnectionError::IO{source: e} if e.kind() == ErrorKind::UnexpectedEof => ConnectionError::NoData(),
                e => e,
            }
        })?;
        let (id, name) = Self::read_game_data(conn).await?;
        Ok(ConnectionHeader {type_, id, name})
    }

    async fn do_read_and_set_connection_header(conn: &mut Connection) -> ConnResult<()>
    {
        let header = Self::read_connection_header(conn).await?;
        conn.set_header(header);
        Ok(())
    }

    pub async fn read_and_set_connection_header(&self, conn: &mut Connection) -> ConnResult<()>
    {
        let c_future = Self::do_read_and_set_connection_header(conn);
        or_conn_shutdown!(c_future, self.shutdown_token)
    }
}
