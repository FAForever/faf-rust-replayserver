use async_std::io::BufReader;
use async_std::io::prelude::BufReadExt;
use async_std::io::prelude::ReadExt;
use async_std::net::TcpStream;
use crate::error::ConnResult;
use crate::error::ConnectionError;
use crate::accept::header::ConnectionHeader;

pub struct Connection
{
    // Unless TcpStream does some forced waiting, this should not block trying to read more data
    // than requested.
    stream: BufReader<TcpStream>,
    header: Option<ConnectionHeader>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Connection { stream: BufReader::new(stream), header: None }
    }

    pub async fn read_exact(&mut self, buf: &mut[u8]) -> ConnResult<()>
    {
        self.stream.read_exact(buf).await.map_err(ConnectionError::from)
    }

    pub async fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>, limit: u64) -> ConnResult<usize>
    {
        self.stream.by_ref().take(limit).read_until(byte, buf).await.map_err(ConnectionError::from)
    }

    pub fn set_header(&mut self, header: ConnectionHeader)
    {
        self.header = Some(header);
    }

    pub fn get_header(&self) -> &Option<ConnectionHeader>
    {
        &(self.header)
    }
}
