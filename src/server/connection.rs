use async_std::net::TcpStream;
pub struct Connection
{
    stream: TcpStream,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Connection { stream }
    }
}
