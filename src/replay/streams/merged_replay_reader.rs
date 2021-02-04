use std::io::{Read, Write};

use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{
    replay::position::StreamPosition, replay::streams::MReplayRef,
    util::buf_traits::DiscontiguousBuf, util::buf_traits::ReadAtExt,
};

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

pub struct MergedReplayReader {
    replay: MReplayRef,
    bytes_read: ReadProgress,
}

impl MergedReplayReader {
    pub fn new(replay: MReplayRef) -> Self {
        Self {
            replay,
            bytes_read: ReadProgress::Header(0),
        }
    }

    // FIXME This is more complex than I'd like.
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
                    self.bytes_read = if l < header.data.len() {
                        ReadProgress::Header(l)
                    } else {
                        ReadProgress::Data(0)
                    };
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

    pub async fn write_to<T: AsyncWrite + Unpin>(&mut self, c: &mut T) -> std::io::Result<()> {
        let mut buf: Box<[u8]> = Box::new([0; 4096]);
        loop {
            let data_read = self.get_data_immediately(&mut *buf);
            if data_read != 0 {
                c.write(&buf[..data_read]).await?;
            } else {
                let has_more_data = self.wait_for_more_data().await;
                if !has_more_data {
                    return Ok(());
                }
            }
        }
    }
}
