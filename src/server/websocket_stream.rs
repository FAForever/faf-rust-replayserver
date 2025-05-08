use futures::{Sink, SinkExt, Stream, StreamExt};
use tokio::{
    io::{AsyncBufRead, AsyncWrite},
    net::TcpStream,
};
use tokio_util::{
    bytes::Bytes,
    io::{CopyToBytes, SinkWriter, StreamReader},
};
use tokio_websockets::{Error, Message, ServerBuilder, WebSocketStream};

use super::connection::{ReaderType, WriterType};

trait WebsocketSink: Sink<Message, Error = Error> {}
impl<T: Sink<Message, Error = Error>> WebsocketSink for T {}

trait WebsocketStream: Stream<Item = Result<Message, Error>> {}
impl<T: Stream<Item = Result<Message, Error>>> WebsocketStream for T {}

async fn make_websocket(stream: TcpStream) -> Result<WebSocketStream<TcpStream>, Error> {
    // Ignore the HTTP stuff
    ServerBuilder::new().accept(stream).await.map(|(_, s)| s)
}

fn split_websocket(socket: WebSocketStream<TcpStream>) -> (impl WebsocketSink, impl WebsocketStream) {
    socket.split()
}

fn map_message_error_to_io_error(e: Error) -> std::io::Error {
    fn other_e(s: impl Into<String>) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, s.into())
    }
    match e {
        Error::AlreadyClosed => other_e("Websocket already closed"),
        Error::CannotResolveHost => other_e("Websocket cannot resolve host"),
        Error::Protocol(protocol_error) => other_e(format!("Websocket protocol error: {}", protocol_error)),
        Error::PayloadTooLong { len, max_len } => other_e(format!("Websocket payload too long: {} > {}", len, max_len)),
        Error::Io(error) => error,
        Error::Upgrade(error) => other_e(format!("Websocket upgrade error: {}", error)),
        _ => other_e("Websocket unknown error"),
    }
}

fn map_websocket_sink_to_bytes(sink: impl WebsocketSink) -> impl for<'a> Sink<&'a [u8], Error = std::io::Error> {
    let bytes_sink = sink
        .with(async |b| Ok(Message::binary(b)))
        .sink_map_err(map_message_error_to_io_error);
    CopyToBytes::new(bytes_sink)
}

fn map_websocket_stream_to_bytes(stream: impl WebsocketStream) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
    stream.map(|i| {
        i.map_err(map_message_error_to_io_error)
            .map(|mes| mes.into_payload().into())
    })
}

fn make_websocket_sink_a_stream(sink: impl WebsocketSink) -> impl AsyncWrite {
    SinkWriter::new(map_websocket_sink_to_bytes(sink))
}

fn make_websocket_stream_a_stream(stream: impl WebsocketStream) -> impl AsyncBufRead {
    StreamReader::new(map_websocket_stream_to_bytes(stream))
}

pub(crate) async fn make_split_websocket_from_tcp(stream: TcpStream) -> Result<(ReaderType, WriterType), Error> {
    let ws = make_websocket(stream).await?;
    let (rw, rr) = split_websocket(ws);
    let r = Box::new(make_websocket_stream_a_stream(rr));
    let w = Box::new(make_websocket_sink_a_stream(rw));
    Ok((r, w))
}
