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
        if line[line.len() - 1] != b'\0' {
            return Err("Connection header is incomplete".into());
        }
        let pieces: Vec<&[u8]> = line[..].splitn(2, |c| c == &b'/').collect();
        if pieces.len() < 2 {
            return Err(ConnectionError::BadData(format!("Connection header has too few slashes: {}", pieces.len())));
        }
        let (id_bytes, name_bytes) = (pieces[0], pieces[1]);
        let name_bytes: &[u8] = &name_bytes[0..name_bytes.len() - 1]; // remove trailing '\0'
        let id = as_conn_err!(u64, from_utf8(id_bytes)?.parse::<u64>()?, "Failed to parse replay ID");
        let name = as_conn_err!(String, String::from(from_utf8(name_bytes)?), "Failed to decode connection string id");
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
    use crate::{server::connection::{test::TestConnection, Connection}, error::ConnectionError};
    use super::{ConnectionHeaderReader, ConnectionType};
    use futures_await_test::async_test;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn setup() -> (Arc<Mutex<TestConnection>>, Connection, StopSource, ConnectionHeaderReader) {
        INIT.call_once(env_logger::init);
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
        assert!(c.get_header().unwrap().type_ == ConnectionType::WRITER);

        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"G/1/foo\0");
        reader.read_and_set_connection_header(&mut c).await.unwrap();
        assert!(c.get_header().unwrap().type_ == ConnectionType::READER);
    }

    #[async_test]
    async fn test_connection_header_invalid_type() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"U/1/foo\0");
        let err = reader.read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[async_test]
    async fn test_connection_header_type_short_data() {
        for short_data in vec![b"" as &[u8], b"P"] {
            let (tc, mut c, _ss, reader) = setup();
            tc.lock().unwrap().append_read_data(short_data);
            let err = reader.read_and_set_connection_header(&mut c).await.err().unwrap();
            assert!(matches!(err, ConnectionError::NoData()));
        }
    }

    #[async_test]
    async fn test_connection_header_replay_info() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"G/1/foo\0");
        reader.read_and_set_connection_header(&mut c).await.unwrap();
        let h = c.get_header().unwrap();
        assert!(h.id == 1);
        assert!(h.name == "foo");
    }

    #[async_test]
    async fn test_connection_header_replay_uid_not_int() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"G/bar/foo\0");
        let err = reader.read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[async_test]
    async fn test_connection_header_replay_info_no_null_end() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"G/1/foo");
        let err = reader.read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[async_test]
    async fn test_connection_header_replay_info_no_delimiter() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"G/1\0");
        let err = reader.read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[async_test]
    async fn test_connection_header_replay_info_more_slashes() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"G/1/name/with/slash\0");
        reader.read_and_set_connection_header(&mut c).await.unwrap();
        let h = c.get_header().unwrap();
        assert!(h.id == 1);
        assert!(h.name == "name/with/slash");
    }

    #[async_test]
    async fn test_connection_header_replay_info_invalid_unicode() {
        let (tc, mut c, _ss, reader) = setup();
        // Lonely start character is invalid unicode
        tc.lock().unwrap().append_read_data(b"G/1/foo \xc0 bar\0");
        let err = reader.read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[async_test]
    async fn test_connection_header_replay_info_limit_overrun() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"G/1/");
        for _ in 0..1024 {
            tc.lock().unwrap().append_read_data(b"foo ");
        }
        tc.lock().unwrap().append_read_data(b"\0");
        let err = reader.read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[async_test]
    async fn test_connection_header_replay_info_negative_id() {
        let (tc, mut c, _ss, reader) = setup();
        tc.lock().unwrap().append_read_data(b"G/-1/foo\0");
        let err = reader.read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    /* TODO test stop token */
    /* TODO implement read timeout */
}
