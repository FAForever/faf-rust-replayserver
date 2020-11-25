use std::io::ErrorKind;
use std::str::from_utf8;

use crate::as_conn_err;
use crate::error::{ConnResult,AddContext};
use crate::error::ConnectionError;
use crate::server::connection::Connection;

pub enum ConnectionType {
    READER = 1,
    WRITER = 2,
}

pub struct ConnectionInfo {
    type_: ConnectionType,
    id: u64,
    name: String,
}

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

pub async fn read_connection_info(conn: &mut Connection) -> ConnResult<ConnectionInfo> // FIXME error type
{
    let type_ = read_type(conn).await.map_err(|e| {
        match e {
            ConnectionError::IO{source: e} if e.kind() == ErrorKind::UnexpectedEof => ConnectionError::NoData(),
            e => e,
        }
    })?;
    let (id, name) = read_game_data(conn).await?;
    Ok(ConnectionInfo {type_, id, name})
}
