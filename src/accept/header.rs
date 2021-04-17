use std::io::ErrorKind;
use std::str::from_utf8;

use tokio::io::AsyncReadExt;
use tokio::time::Duration;

use crate::error::bad_data;
use crate::error::ConnResult;
use crate::error::ConnectionError;
use crate::error::SomeError;
use crate::server::connection::Connection;
use crate::util::timeout::timeout;
use crate::{server::connection::read_until_exact, some_error};

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ConnectionType {
    Reader = 1,
    Writer = 2,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ConnectionHeader {
    pub type_: ConnectionType,
    pub id: u64,
    pub name: String,
}

pub mod header_reader {
    use super::*;

    async fn read_type(conn: &mut Connection) -> ConnResult<ConnectionType> {
        let mut buf: [u8; 2] = [0; 2];
        conn.read_exact(&mut buf).await?;
        match &buf {
            b"P/" => Ok(ConnectionType::Writer),
            b"G/" => Ok(ConnectionType::Reader),
            _ => Err(bad_data(format!("Invalid connection type: {:x?}", buf))),
        }
    }

    async fn read_game_data(conn: &mut Connection) -> ConnResult<(u64, String)> {
        let mut line = Vec::<u8>::new();
        read_until_exact(&mut conn.take(1024), b'\0', &mut line)
            .await
            .map_err(|_| bad_data("Connection header is incomplete"))?;

        let pieces: Vec<&[u8]> = line[..].splitn(2, |c| c == &b'/').collect();
        if pieces.len() < 2 {
            return Err(bad_data(format!(
                "Connection header has too few slashes: {}",
                pieces.len()
            )));
        }
        let (id_bytes, name_bytes) = (pieces[0], pieces[1]);
        let name_bytes: &[u8] = &name_bytes[0..name_bytes.len() - 1]; // remove trailing '\0'

        let id = some_error!(from_utf8(id_bytes)?.parse::<u64>()?)
            .map_err(|_| bad_data("Failed to parse replay ID"))?;
        let name = some_error!(String::from(from_utf8(name_bytes)?))
            .map_err(|_| bad_data("Failed to decode connection string id"))?;
        Ok((id, name))
    }

    async fn read_connection_header(conn: &mut Connection) -> ConnResult<ConnectionHeader> {
        let type_ = read_type(conn).await.map_err(|e| match e {
            ConnectionError::IO { source: e, .. } if e.kind() == ErrorKind::UnexpectedEof => {
                ConnectionError::NoData
            }
            e => e,
        })?;
        let (id, name) = read_game_data(conn).await?;
        Ok(ConnectionHeader { type_, id, name })
    }

    /* Cancellable. */
    pub async fn read_and_set_connection_header(conn: &mut Connection) -> ConnResult<()> {
        let header = read_connection_header(conn).await?;
        conn.set_header(header);
        Ok(())
    }
}

pub async fn read_initial_header(conn: &mut Connection, until: Duration) -> ConnResult<()> {
    match timeout(header_reader::read_and_set_connection_header(conn), until).await {
        Some(res) => res,
        None => Err(bad_data("Timed out while accepting connection")),
    }
}

#[cfg(test)]
mod test {
    use super::header_reader::read_and_set_connection_header;
    use super::*;
    use crate::util::test::setup_logging;
    use crate::{error::ConnectionError, server::connection::Connection};
    use std::io::Cursor;
    use tokio::io::BufReader;

    fn conn_from_read_data(data: &'static [u8]) -> Connection {
        let r = Box::new(BufReader::new(Cursor::new(data)));
        Connection::new_from(r, Box::new(tokio::io::sink()))
    }

    #[tokio::test]
    async fn test_connection_header_type() {
        setup_logging();
        let mut c = conn_from_read_data(b"P/1/foo\0");
        read_and_set_connection_header(&mut c).await.unwrap();
        assert!(c.get_header().type_ == ConnectionType::Writer);

        c = conn_from_read_data(b"G/1/foo\0");
        read_and_set_connection_header(&mut c).await.unwrap();
        assert!(c.get_header().type_ == ConnectionType::Reader);
    }

    #[tokio::test]
    async fn test_connection_header_invalid_type() {
        setup_logging();
        let mut c = conn_from_read_data(b"U/1/foo\0");
        let err = read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_type_short_data() {
        for short_data in vec![b"" as &[u8], b"P"] {
            setup_logging();
            let mut c = conn_from_read_data(short_data);
            let err = read_and_set_connection_header(&mut c).await.err().unwrap();
            assert!(matches!(err, ConnectionError::NoData));
        }
    }

    #[tokio::test]
    async fn test_connection_header_replay_info() {
        setup_logging();
        let mut c = conn_from_read_data(b"G/1/foo\0");
        read_and_set_connection_header(&mut c).await.unwrap();
        let h = c.get_header();
        assert!(h.id == 1);
        assert!(h.name == "foo");
    }

    #[tokio::test]
    async fn test_connection_header_replay_uid_not_int() {
        setup_logging();
        let mut c = conn_from_read_data(b"G/bar/foo\0");
        let err = read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_no_null_end() {
        setup_logging();
        let mut c = conn_from_read_data(b"G/1/foo");
        let err = read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_no_delimiter() {
        setup_logging();
        let mut c = conn_from_read_data(b"G/1\0");
        let err = read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_more_slashes() {
        setup_logging();
        let mut c = conn_from_read_data(b"G/1/name/with/slash\0");
        read_and_set_connection_header(&mut c).await.unwrap();
        let h = c.get_header();
        assert!(h.id == 1);
        assert!(h.name == "name/with/slash");
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_invalid_unicode() {
        setup_logging();
        // Lonely start character is invalid unicode
        let mut c = conn_from_read_data(b"G/1/foo \xc0 bar\0");
        let err = read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_limit_overrun() {
        setup_logging();
        let mut data: Vec<u8> = b"G/1/".to_vec();

        for _ in 0..1024 {
            data.extend(b"foo ");
        }
        data.extend(b"\0");
        let mut c = conn_from_read_data(data.leak());
        let err = read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }

    #[tokio::test]
    async fn test_connection_header_replay_info_negative_id() {
        setup_logging();
        let mut c = conn_from_read_data(b"G/-1/foo\0");
        let err = read_and_set_connection_header(&mut c).await.err().unwrap();
        assert!(matches!(err, ConnectionError::BadData(..)));
    }
}
