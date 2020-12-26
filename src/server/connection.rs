use tokio::{io::BufReader, net::TcpStream};
use tokio::io::AsyncBufReadExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::io::AsyncReadExt;
use crate::error::ConnResult;
use crate::error::ConnectionError;
use crate::accept::header::ConnectionHeader;

// TODO support writing

#[cfg_attr(test, faux::create)]
pub struct Connection
{
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    header: Option<ConnectionHeader>,
}

#[cfg_attr(test, faux::methods)]
impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        let (r, writer) = stream.into_split();
        let reader = BufReader::new(r);
        Self {reader, writer, header: None }
    }
    /* Unlike in Python, reaching the end without encountering the delimiter is a success. */
    pub async fn read_exact(&mut self, buf: &mut[u8]) -> ConnResult<()>
    {
        self.reader.read_exact(buf).await.map(|_| ()).map_err(ConnectionError::from)
    }

    pub async fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>, limit: u64) -> ConnResult<usize>
    {
        (&mut self.reader).take(limit).read_until(byte, buf).await.map_err(ConnectionError::from)
    }

    pub fn set_header(&mut self, header: ConnectionHeader)
    {
        self.header = Some(header);
    }

    pub fn get_header(&self) -> Option<ConnectionHeader>
    {
        self.header.clone()
    }
}

#[cfg(test)]
pub mod test {
    use std::{io::{Cursor, Read, BufRead}, sync::Arc, sync::Mutex, io::Write, io::Seek, io::SeekFrom};
    use crate::{error::{ConnResult, ConnectionError}, accept::header::ConnectionHeader};
    use super::Connection;

    /* We can't reuse the regular connection because you can't pass async fns to faux yet */
    pub struct TestConnection {
        stream: Cursor<Vec<u8>>,
        header: Option<ConnectionHeader>,
    }
    impl TestConnection {
        pub fn new() -> Self {
            let v = Cursor::new(Vec::new());
            Self { stream: v, header: None}
        }

        pub fn read_exact(&mut self, buf: &mut[u8]) -> ConnResult<()>
        {
            self.stream.read_exact(buf).map_err(ConnectionError::from)
        }

        pub fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>, limit: u64) -> ConnResult<usize>
        {
            Read::by_ref(&mut self.stream).take(limit).read_until(byte, buf).map_err(ConnectionError::from)
        }

        pub fn set_header(&mut self, header: ConnectionHeader)
        {
            self.header = Some(header);
        }

        pub fn get_header(&self) -> Option<ConnectionHeader>
        {
            self.header.clone()
        }

        pub fn append_read_data(&mut self, buf: &[u8])
        {
            let rpos = self.stream.position();
            self.stream.seek(SeekFrom::End(0)).unwrap();
            self.stream.write(buf).unwrap();
            self.stream.seek(SeekFrom::Start(rpos)).unwrap();
        }

        pub fn faux() -> (Arc<Mutex<TestConnection>>, Connection) {
            let tc = Arc::new(Mutex::new(TestConnection::new()));
            let mut c = Connection::faux();
            unsafe {
                let tcc = tc.clone();
                faux::when!(c.read_exact).then(move |buf| tcc.lock().unwrap().read_exact(buf));
                let tcc = tc.clone();
                faux::when!(c.read_until).then(move |(byte, buf, limit)| tcc.lock().unwrap().read_until(byte, buf, limit));
                let tcc = tc.clone();
                faux::when!(c.get_header).then(move |()| tcc.lock().unwrap().get_header());
                let tcc = tc.clone();
                faux::when!(c.set_header).then(move |h| tcc.lock().unwrap().set_header(h));
            }
            (tc, c)
        }
    }
}
