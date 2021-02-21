use std::io::ErrorKind;
use std::str::from_utf8;

use tokio::io::AsyncReadExt;

use crate::error::ConnResult;
use crate::error::ConnectionError;
use crate::error::SomeError;
use crate::server::connection::Connection;
use crate::{server::connection::read_until_exact, some_error};

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

pub struct ConnectionHeaderReader {}

impl ConnectionHeaderReader {
    pub fn new() -> Self {
        Self {}
    }

    async fn read_type(conn: &mut Connection) -> ConnResult<ConnectionType> {
        let mut buf: [u8; 2] = [0; 2];
        conn.read_exact(&mut buf).await?;
        match &buf {
            b"P/" => Ok(ConnectionType::WRITER),
            b"G/" => Ok(ConnectionType::READER),
            _ => Err(format!("Invalid connection type: {:x?}", buf).into()),
        }
    }

    async fn read_game_data(conn: &mut Connection) -> ConnResult<(u64, String)> {
        let mut line = Vec::<u8>::new();
        read_until_exact(&mut conn.take(1024), b'\0', &mut line)
            .await
            .map_err(|_| ConnectionError::bad_data("Connection header is incomplete"))?;

        let pieces: Vec<&[u8]> = line[..].splitn(2, |c| c == &b'/').collect();
        if pieces.len() < 2 {
            return Err(format!("Connection header has too few slashes: {}", pieces.len()).into());
        }
        let (id_bytes, name_bytes) = (pieces[0], pieces[1]);
        let name_bytes: &[u8] = &name_bytes[0..name_bytes.len() - 1]; // remove trailing '\0'

        let id = some_error!(from_utf8(id_bytes)?.parse::<u64>()?)
            .map_err(|_| ConnectionError::bad_data("Failed to parse replay ID"))?;
        let name = some_error!(String::from(from_utf8(name_bytes)?))
            .map_err(|_| ConnectionError::bad_data("Failed to decode connection string id"))?;
        Ok((id, name))
    }

    async fn read_connection_header(conn: &mut Connection) -> ConnResult<ConnectionHeader> {
        let type_ = Self::read_type(conn).await.map_err(|e| match e {
            ConnectionError::IO { source: e, .. } if e.kind() == ErrorKind::UnexpectedEof => {
                ConnectionError::NoData()
            }
            e => e,
        })?;
        let (id, name) = Self::read_game_data(conn).await?;
        Ok(ConnectionHeader { type_, id, name })
    }

    /* Cancellable. */
    pub async fn read_and_set_connection_header(&self, conn: &mut Connection) -> ConnResult<()> {
        let header = Self::read_connection_header(conn).await?;
        conn.set_header(header);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{ConnectionHeaderReader, ConnectionType};
    use crate::{error::ConnectionError, server::connection::Connection};
    use std::{io::Cursor, sync::Once};
    use tokio::io::BufReader;

    static INIT: Once = Once::new();

    fn setup() {
        INIT.call_once(env_logger::init);
    }

    fn conn_from_read_data(data: &'static [u8]) -> Connection {
        let r = Box::new(BufReader::new(Cursor::new(data)));
        Connection::test(r, Box::new(tokio::io::sink()))
    }

    #[tokio::test]
    async fn test_connection_header_type() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut c = conn_from_read_data(b"P/1/foo\0");
        reader.read_and_set_connection_header(&mut c).await.unwrap();
        assert!(c.get_header().type_ == ConnectionType::WRITER);

        c = conn_from_read_data(b"G/1/foo\0");
        reader.read_and_set_connection_header(&mut c).await.unwrap();
        assert!(c.get_header().type_ == ConnectionType::READER);
    }

    #[tokio::test]
    async fn test_connection_header_invalid_type() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut c = conn_from_read_data(b"U/1/foo\0");
        let err = reader
            .read_and_set_connection_header(&mut c)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_type_short_data() {
        for short_data in vec![b"" as &[u8], b"P"] {
            setup();
            let reader = ConnectionHeaderReader::new();
            let mut c = conn_from_read_data(short_data);
            let err = reader
                .read_and_set_connection_header(&mut c)
                .await
                .err()
                .unwrap();
            assert!(matches!(err, ConnectionError::NoData()));
        }
    }

    #[tokio::test]
    async fn test_connection_header_replay_info() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut c = conn_from_read_data(b"G/1/foo\0");
        reader.read_and_set_connection_header(&mut c).await.unwrap();
        let h = c.get_header();
        assert!(h.id == 1);
        assert!(h.name == "foo");
    }

    #[tokio::test]
    async fn test_connection_header_replay_uid_not_int() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut c = conn_from_read_data(b"G/bar/foo\0");
        let err = reader
            .read_and_set_connection_header(&mut c)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_no_null_end() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut c = conn_from_read_data(b"G/1/foo");
        let err = reader
            .read_and_set_connection_header(&mut c)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_no_delimiter() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut c = conn_from_read_data(b"G/1\0");
        let err = reader
            .read_and_set_connection_header(&mut c)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_more_slashes() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut c = conn_from_read_data(b"G/1/name/with/slash\0");
        reader.read_and_set_connection_header(&mut c).await.unwrap();
        let h = c.get_header();
        assert!(h.id == 1);
        assert!(h.name == "name/with/slash");
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_invalid_unicode() {
        setup();
        let reader = ConnectionHeaderReader::new();
        // Lonely start character is invalid unicode
        let mut c = conn_from_read_data(b"G/1/foo \xc0 bar\0");
        let err = reader
            .read_and_set_connection_header(&mut c)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_limit_overrun() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut data: Vec<u8> = b"G/1/".to_vec();

        for _ in 0..1024 {
            data.extend(b"foo ");
        }
        data.extend(b"\0");
        let mut c = conn_from_read_data(data.leak());
        let err = reader
            .read_and_set_connection_header(&mut c)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_negative_id() {
        setup();
        let reader = ConnectionHeaderReader::new();
        let mut c = conn_from_read_data(b"G/-1/foo\0");
        let err = reader
            .read_and_set_connection_header(&mut c)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }
}
