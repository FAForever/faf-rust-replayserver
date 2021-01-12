use std::{collections::VecDeque, cmp::min};
use std::io::Write;

use super::buf_traits::{BufWithDiscard, DiscontiguousBuf};

const CHUNK_SIZE: usize = 4096;

/* Buffer deque with discarding.
 * INVARIANTS:
 * * Total chunk length = max(0, end - discard_start).
 * * Each chunk is aligned to CHUNK_SIZE bytes.
 * * There are no empty chunks.
 * */
pub struct BufDeque {
    chunks: VecDeque<Box<[u8; CHUNK_SIZE]>>,
    discard_start: usize,
    end: usize,
}

impl BufDeque {
    pub fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            discard_start: 0,
            end: 0,
        }
    }

    fn get_last_chunk(&mut self) -> &mut [u8; CHUNK_SIZE] {
        if self.no_space_in_last_chunk() {
            let new_chunk = Box::new([0; CHUNK_SIZE]);
            self.chunks.push_back(new_chunk);
        }
        debug_assert!(!self.chunks.is_empty());
        &mut **self.chunks.back_mut().unwrap()
    }

    fn no_space_in_last_chunk(&self) -> bool {
        self.end % CHUNK_SIZE == 0 || self.discard_start >= self.end
    }

    fn chunk(&self, start: usize) -> (usize, usize, usize) {
        assert!(self.discard_start <= start && start < self.end);

        let first_chunk_idx = self.discard_start / CHUNK_SIZE;
        let target_chunk_idx = start / CHUNK_SIZE;
        let rel_idx = target_chunk_idx - first_chunk_idx;

        let chunk_start = start % CHUNK_SIZE;
        let chunk_end = min(CHUNK_SIZE, self.end - (target_chunk_idx * CHUNK_SIZE));

        (rel_idx, chunk_start, chunk_end)
    }
}

impl Write for BufDeque {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let offset = self.end % CHUNK_SIZE;
        let mut chunk: &mut [u8] = self.get_last_chunk();
        chunk = &mut chunk[offset..];
        let written = chunk.write(buf).unwrap();
        self.end += written;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl DiscontiguousBuf for BufDeque {
    fn len(&self) -> usize {
        self.end
    }

    /* We slightly break contract here and panic if discarded data is accessed. */
    fn get_chunk(&self, start: usize) -> &[u8] {
        let (idx, start, end) = self.chunk(start);
        assert!(self.discard_start <= start && start < self.end);
        let chunk = &**self.chunks.get(idx).unwrap();
        &chunk[start..end]
    }

    fn get_mut_chunk(&mut self, start: usize) -> &mut [u8] {
        let (idx, start, end) = self.chunk(start);
        assert!(self.discard_start <= start && start < self.end);
        let chunk = &mut **self.chunks.get_mut(idx).unwrap();
        &mut chunk[start..end]
    }
}

impl BufWithDiscard for BufDeque {
    /* TODO - shrink_to_fit on vecdeque? It'll never be large anyway */
    fn discard(&mut self, until: usize) {
        if until <= self.discard_start {
            return
        }

        let until_chunk_idx = until / CHUNK_SIZE;
        let mut first_chunk_idx = self.discard_start / CHUNK_SIZE;
        while !self.chunks.is_empty() && first_chunk_idx < until_chunk_idx {
            self.chunks.pop_front();
            first_chunk_idx += 1;
        }
        self.discard_start = until;
    }
}

#[cfg(test)]
mod test {
    use std::io::Write;
    use crate::async_utils::buf_traits::{DiscontiguousBuf, BufWithDiscard};
    use super::{BufDeque, CHUNK_SIZE};

    fn append_at(offset: usize) {
        let off: Box<[u8]> = vec![0 as u8;offset].into();
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
            assert_eq!(bl.end, (i+ 1) * data.len());
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
            while cursor < (i + 1) * data.len() {
                let chunk = bl.get_chunk(cursor);
                for j in 0..chunk.len() {
                    assert_eq!(chunk[j], data[(cursor + j) % data.len()]);
                }
                cursor += chunk.len();
            }
            bl.discard((i + 1) * data.len());
        }
    }
}
