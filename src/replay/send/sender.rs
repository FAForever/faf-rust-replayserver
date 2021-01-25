use std::io::{Read, Write};

use tokio::{io::AsyncWriteExt, select};
use tokio_util::sync::CancellationToken;

use crate::{server::connection::Connection, replay::streams::MReplayRef, replay::position::StreamPosition, util::buf_traits::ReadAtExt, util::buf_traits::DiscontiguousBuf};

enum ReadProgress {
    Header(usize),
    Data(usize),
}

impl ReadProgress {
    pub fn needed_stream_position(&self) -> StreamPosition {
        match self {
            Self::Header(..) => StreamPosition::HEADER,
            Self::Data(l) => StreamPosition::DATA(*l),
        }
    }
}

struct MergedReplayReader {
    replay: MReplayRef,
    bytes_read: ReadProgress,
}

impl MergedReplayReader {
    fn new(replay: MReplayRef) -> Self {
        Self {
            replay,
            bytes_read: ReadProgress::Header(0),
        }
    }

    fn get_data_immediately(&mut self, mut buf: &mut [u8]) -> usize {
        let replay = self.replay.borrow();
        if self.bytes_read.needed_stream_position() > replay.position() {
            return 0;
        }
        match self.bytes_read {
            ReadProgress::Header(mut l) => {
                let header = replay.get_header().unwrap();
                if l >= header.data.len() {
                    0
                } else {
                    let data_written = buf.write(&header.data[l..]).unwrap();
                    l += data_written;
                    self.bytes_read = if l < header.data.len() { ReadProgress::Header(l) } else  { ReadProgress::Data(0) };
                    data_written
                }
            }
            ReadProgress::Data(l) => {
                let data = replay.get_data();
                if l >= data.len() {
                    0
                } else {
                    let mut new_replay_data = data.reader_from(l);
                    let read_bytes = new_replay_data.read(buf).unwrap();
                    self.bytes_read = ReadProgress::Data(l + read_bytes);
                    read_bytes
                }
            }
        }
    }

    async fn wait_for_more_data(&self) -> bool {
        let replay_position = self.replay.borrow().position();
        let next_position = match replay_position {
            StreamPosition::START => Some(StreamPosition::HEADER),
            StreamPosition::HEADER => Some(StreamPosition::DATA(1)),
            StreamPosition::DATA(l) => Some(StreamPosition::DATA(l + 1)),
            StreamPosition::FINISHED(..) => None,
        };
        match next_position {
            None => false,
            Some(p) => {
                let f = self.replay.borrow().wait(p);
                f.await;
                true
            }
        }
    }

    pub async fn send_replay(&mut self, c: &mut Connection) {
        let mut buf: Box<[u8]> = Box::new([0; 4096]);
        loop {
            let data_read = self.get_data_immediately(&mut *buf);
            if data_read != 0 {
                if let Err(_) = c.write(&buf[..data_read]).await {
                    /* FIXME log */
                    return;
                }
            } else {
                let has_more_data = self.wait_for_more_data().await;
                if !has_more_data {
                    return;
                }
            }
        }
    }
}

pub struct ReplaySender {
    merged_replay: MReplayRef,
    shutdown_token: CancellationToken,
}

impl ReplaySender {
    pub fn new(merged_replay: MReplayRef, shutdown_token: CancellationToken) -> Self {
        Self { merged_replay, shutdown_token }
    }

    pub async fn handle_connection(&self, c: &mut Connection) {
        select! {
            _ = self.send_replay_to_connection(c) => (),
            _ = self.shutdown_token.cancelled() => (),
        }
    }

    async fn send_replay_to_connection(&self, c: &mut Connection) {
        let mut reader = MergedReplayReader::new(self.merged_replay.clone());
        reader.send_replay(c).await;
    }
}
