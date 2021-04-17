use std::io::Write;
use std::{cmp::min, collections::VecDeque};

use super::buf_traits::DiscontiguousBuf;

const CHUNK_SIZE: usize = 4096;

/* Buffer deque with discarding.
 * TODO think about a way to do zero-copy stuff.
 */
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
        assert!(self.discard_start() <= self.end);
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
        assert!(self.discard_start() <= self.end);
        self.relative_end() == self.chunks.len() * CHUNK_SIZE
    }

    /* Discard data at most up to 'until'.
     * Can discard beyond data end, in that case new writes will only increase the end value until
     * it reaches discard point.
     * */
    pub fn discard(&mut self, until: usize) {
        let discardable_chunks = until / CHUNK_SIZE;
        while self.discarded_chunks < discardable_chunks && !self.chunks.is_empty() {
            self.chunks.pop_front();
            self.discarded_chunks += 1;
        }
        self.discarded_chunks = std::cmp::max(discardable_chunks, self.discarded_chunks);
    }

    fn append_some(&mut self, mut buf: &[u8]) -> usize {
        let mut written = 0;
        let mut skipped = 0;

        // If we discarded beyond end, cut off the start
        if self.end < self.discard_start() {
            let data_discarded_beyond_end = self.discard_start() - self.end;
            let bytes_to_skip = std::cmp::min(data_discarded_beyond_end, buf.len());
            skipped = bytes_to_skip;
            self.end += skipped;
            buf = &buf[bytes_to_skip..];
        }

        // If we still have bytes to write, write them
        if !buf.is_empty() {
            let offset = self.end % CHUNK_SIZE;
            let mut chunk: &mut [u8] = self.get_last_chunk();
            chunk = &mut chunk[offset..];
            written = chunk.write(buf).unwrap();
            self.end += written;
        }
        skipped + written
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
}

impl Write for BufDeque {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(self.append_some(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{BufDeque, CHUNK_SIZE};
    use crate::util::buf_traits::{DiscontiguousBuf, ReadAtExt};
    use std::io::{Read, Write};

    fn compare_eq(deque: &BufDeque, data: &Vec<u8>) {
        let mut deque_data = Vec::new();
        deque.reader().read_to_end(&mut deque_data).unwrap();
        assert_eq!(&deque_data, data);
    }
    fn append_at(offset: usize) {
        let mut off: Vec<u8> = vec![0 as u8; offset];
        let mut data = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let mut bl = BufDeque::new();
        bl.write_all(&*off).unwrap();
        bl.write_all(&data).unwrap();
        off.append(&mut data);
        compare_eq(&bl, &off);
    }

    #[test]
    fn test_append() {
        append_at(0);
        append_at(CHUNK_SIZE * 2 - 4);
        append_at(CHUNK_SIZE * 2);
        append_at(CHUNK_SIZE * 2 + 4);
    }

    #[test]
    fn test_get_chunk() {
        let mut bl = BufDeque::new();
        let data = vec![0, 1, 2, 3, 4, 5, 6];
        for i in 0..(CHUNK_SIZE * 8) {
            bl.write_all(&data).unwrap();
            assert_eq!(bl.len(), (i + 1) * data.len());
        }

        let mut cursor = 0;
        while cursor < CHUNK_SIZE * 8 * data.len() {
            let chunk = bl.get_chunk(cursor);
            for i in 0..chunk.len() {
                assert_eq!(chunk[i], data[(cursor + i) % data.len()]);
            }
            cursor += chunk.len();
        }
    }

    #[test]
    fn test_discard_data_immediately() {
        let mut bl = BufDeque::new();
        let data = vec![0, 1, 2, 3, 4, 5, 6];
        for i in 0..(CHUNK_SIZE * 8) {
            bl.write_all(&data).unwrap();
            let mut read = Vec::new();
            bl.reader_from(i * data.len()).read_to_end(&mut read).unwrap();
            assert_eq!(data, read);
            bl.discard((i + 1) * data.len());
        }
    }

    #[test]
    fn test_chunks_are_discarded_beyond_end() {
        let mut bl = BufDeque::new();
        let data = [1; CHUNK_SIZE];
        let data2 = [1; CHUNK_SIZE / 2];
        bl.write_all(&data).unwrap();

        bl.discard(CHUNK_SIZE * 2);
        assert!(bl.chunks.is_empty());
        bl.write_all(&data2).unwrap();
        assert!(bl.chunks.is_empty());
        bl.write_all(&data2).unwrap();
        assert!(bl.chunks.is_empty());
    }

    #[test]
    fn test_partial_write_after_discard() {
        let mut bl = BufDeque::new();
        let data1 = [1; CHUNK_SIZE - 4];
        let data2 = [2; 8];
        bl.write_all(&data1).unwrap();

        bl.discard(CHUNK_SIZE);
        bl.write_all(&data2).unwrap();
        let mut read = Vec::new();
        bl.reader_from(CHUNK_SIZE).read_to_end(&mut read).unwrap();
        assert_eq!(read, vec![2; 4]);
    }
}
