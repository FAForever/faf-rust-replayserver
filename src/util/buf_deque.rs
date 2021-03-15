use std::io::Write;
use std::{cmp::min, collections::VecDeque};

use super::buf_traits::{BufWithDiscard, DiscontiguousBuf, DiscontiguousBufExt};

const CHUNK_SIZE: usize = 4096;

/* Buffer deque with discarding.
 * INVARIANTS:
 * * Total chunk length = max(0, end - discard_start).
 * * Each chunk is aligned to CHUNK_SIZE bytes.
 * * There are no empty chunks.
 * */
pub struct BufDeque {
    chunks: VecDeque<Box<[u8; CHUNK_SIZE]>>,
    discarded_chunks: usize,
    end: usize,
}

impl BufDeque {
    pub fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            discarded_chunks: 0,
            end: 0,
        }
    }

    fn discard_start(&self) -> usize {
        self.discarded_chunks * CHUNK_SIZE
    }

    fn relative_end(&self) -> usize {
        self.end - self.discard_start()
    }

    fn get_last_chunk(&mut self) -> &mut [u8; CHUNK_SIZE] {
        if self.no_space_in_last_chunk() {
            let new_chunk = Box::new([0; CHUNK_SIZE]);
            self.chunks.push_back(new_chunk);
        }
        &mut **self.chunks.back_mut().unwrap()
    }

    fn no_space_in_last_chunk(&self) -> bool {
        self.end % CHUNK_SIZE == 0 || self.chunks.is_empty()
    }
}

impl DiscontiguousBuf for BufDeque {
    fn len(&self) -> usize {
        self.end
    }

    /* We break contract here and potentially panic if discarded data is accessed. */
    fn get_chunk(&self, mut start: usize) -> &[u8] {
        assert!(self.discard_start() <= start && start < self.end);
        start -= self.discard_start();

        let chunk_idx = start / CHUNK_SIZE;
        let chunk_start = start % CHUNK_SIZE;
        let chunk_end = min(CHUNK_SIZE, self.relative_end() - (chunk_idx * CHUNK_SIZE));

        let chunk = &**self.chunks.get(chunk_idx).unwrap();
        &chunk[chunk_start..chunk_end]
    }

    fn append_some(&mut self, buf: &[u8]) -> usize {
        let offset = self.end % CHUNK_SIZE;
        let mut chunk: &mut [u8] = self.get_last_chunk();
        chunk = &mut chunk[offset..];
        let written = chunk.write(buf).unwrap();
        self.end += written;
        written
    }
}

impl Write for BufDeque {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.append(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl BufWithDiscard for BufDeque {
    fn discard(&mut self, until: usize) {
        let until_chunk = until / CHUNK_SIZE;
        // Always keep at least one chunk.
        while self.chunks.len() > 1 && self.discarded_chunks < until_chunk {
            self.chunks.pop_front();
            self.discarded_chunks += 1;
        }
    }
}

#[cfg(test)]
mod test {
    use super::{BufDeque, CHUNK_SIZE};
    use crate::util::buf_traits::{BufWithDiscard, DiscontiguousBuf};
    use std::io::Write;

    fn append_at(offset: usize) {
        let off: Box<[u8]> = vec![0 as u8; offset].into();
        let mut bl = BufDeque::new();
        let data = [0, 1, 2, 3, 4, 5, 6, 7];
        let total_len = offset + data.len();
        bl.write_all(&*off).unwrap();
        bl.write_all(&data).unwrap();
        assert_eq!(bl.end, total_len);

        let mut cursor = off.len();
        while cursor < total_len {
            let chunk = bl.get_chunk(cursor);
            assert!(cursor + chunk.len() <= total_len);
            for i in 0..chunk.len() {
                assert_eq!(data[cursor - off.len() + i], chunk[i]);
            }
            cursor += chunk.len();
        }
    }

    #[test]
    fn test_append() {
        append_at(0);
        append_at(CHUNK_SIZE * 2 - 4);
        append_at(CHUNK_SIZE * 2);
        append_at(CHUNK_SIZE * 2 + 4);
    }

    #[test]
    fn simple_test() {
        let mut bl = BufDeque::new();
        let data = [0, 1, 2, 3, 4, 5, 6, 7];
        for i in 0..(CHUNK_SIZE * 3) {
            bl.write_all(&data).unwrap();
            assert_eq!(bl.end, (i + 1) * data.len());
        }

        let mut cursor = 0;
        while cursor < CHUNK_SIZE * 3 * data.len() {
            let chunk = bl.get_chunk(cursor);
            for i in 0..chunk.len() {
                assert_eq!(chunk[i], data[(cursor + i) % data.len()]);
            }
            cursor += chunk.len();
        }
    }

    #[test]
    fn test_remove_immediately() {
        let mut bl = BufDeque::new();
        let data = [0, 1, 2, 3, 4, 5, 6, 7];
        let mut cursor = 0;
        for i in 0..(CHUNK_SIZE * 3) {
            bl.write_all(&data).unwrap();
            let end = (i + 1) * data.len();
            while cursor < end {
                let chunk = bl.get_chunk(cursor);
                for j in 0..std::cmp::min(chunk.len(), end - cursor) {
                    assert_eq!(chunk[j], data[(cursor + j) % data.len()]);
                }
                cursor += chunk.len();
            }
            bl.discard((i + 1) * data.len());
        }
    }
}
