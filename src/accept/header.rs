use std::fmt::Formatter;
use std::io::ErrorKind;
use std::str::from_utf8;
use stop_token::StopToken;

use crate::as_conn_err;
use crate::error::{ConnResult,AddContext};
use crate::error::ConnectionError;
use crate::or_conn_shutdown;
use crate::server::connection::Connection;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ConnectionType {
    READER = 1,
    WRITER = 2,
}

#[derive(PartialEq, Eq, Debug, Clone)]
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

        let id_bytes = pieces[0];
        let name_bytes = pieces[1];
        let name_bytes: &[u8] = &name_bytes[0..name_bytes.len() - 1];
        let id = as_conn_err!(u64, from_utf8(id_bytes)?.parse::<u64>()?,
        "Failed to parse replay ID");
        let name = as_conn_err!(String, String::from(from_utf8(name_bytes)?),
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

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};
    use stop_token::StopSource;
    use crate::server::connection::{test::TestConnection, Connection};
    use super::{ConnectionHeaderReader, ConnectionType, ConnectionHeader};
    use futures_await_test::async_test;

    fn setup() -> (Arc<Mutex<TestConnection>>, Connection, StopSource, ConnectionHeaderReader) {
        env_logger::init();
        let (tc, c) = TestConnection::faux();
        let ss = StopSource::new();
        let reader = ConnectionHeaderReader::new(ss.stop_token());
        (tc, c, ss, reader)
    }

    #[async_test]
    async fn test_connection_header_type() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"P/1/foo\0");
        reader.read_and_set_connection_header(&mut c).await.unwrap();
        assert!(c.get_header() == Some(ConnectionHeader {
            type_: ConnectionType::WRITER,
            id: 1,
            name: "foo".into()}));
    }
}
