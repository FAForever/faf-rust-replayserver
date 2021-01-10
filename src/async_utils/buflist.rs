use std::{collections::VecDeque, cmp::min};
use std::io::Write;

use super::buf_with_discard::BufWithDiscard;

const CHUNK_SIZE: usize = 4096;

/* Buffer deque with discarding.
 * INVARIANTS:
 * * Total chunk length = max(0, end - discard_start).
 * * Each chunk is aligned to CHUNK_SIZE bytes.
 * * There are no empty chunks.
 * */
pub struct BufList {
    chunks: VecDeque<Box<[u8; CHUNK_SIZE]>>,
    discard_start: usize,
    end: usize,
}

impl BufList {
    pub fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            discard_start: 0,
            end: 0,
        }
    }

    fn append_some(&mut self, buf: &[u8]) -> usize {
        let offset = self.end % CHUNK_SIZE;
        let mut chunk: &mut [u8] = self.get_last_chunk();
        chunk = &mut chunk[offset..];
        let written = chunk.write(buf).unwrap();
        self.end += written;
        written
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
}

impl BufWithDiscard for BufList {
    fn discard_start(&self) -> usize {
        self.discard_start
    }
    fn end(&self) -> usize {
        self.end
    }
    fn append(&mut self, buf: &[u8]) {
        let old_end = self.end;
        let mut cursor = 0;

        if self.discard_start > self.end {
            cursor = self.discard_start - self.end;
        }
        // Adjust end for the buffer part we skipped
        cursor = min(cursor, buf.len());
        self.end += cursor;

        while cursor < buf.len() {
            cursor += self.append_some(&buf[cursor..]);
        }

        debug_assert_eq!(old_end + buf.len(), self.end);
    }

    fn get_chunk(&self, start: usize) -> &[u8] {
        assert!(self.discard_start <= start && start < self.end);
        let first_chunk_idx = self.discard_start / CHUNK_SIZE;
        let target_chunk_idx = start / CHUNK_SIZE;
        let rel_idx = target_chunk_idx - first_chunk_idx;
        let whole_chunk = &**self.chunks.get(rel_idx).unwrap();

        let chunk_start = start % CHUNK_SIZE;
        let chunk_end = min(CHUNK_SIZE, self.end - (target_chunk_idx * CHUNK_SIZE));
        let res = &whole_chunk[chunk_start..chunk_end];

        debug_assert!(start + res.len() <= self.end);
        res
    }

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
    use super::{BufList, CHUNK_SIZE, BufWithDiscard};

    fn append_at(offset: usize) {
        let off: Box<[u8]> = vec![0 as u8;offset].into();
        let mut bl = BufList::new();
        let data = [0, 1, 2, 3, 4, 5, 6, 7];
        let total_len = offset + data.len();
        bl.append(&*off);
        bl.append(&data);
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
        let mut bl = BufList::new();
        let data = [0, 1, 2, 3, 4, 5, 6, 7];
        for i in 0..(CHUNK_SIZE * 3) {
            bl.append(&data);
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
        let mut bl = BufList::new();
        let data = [0, 1, 2, 3, 4, 5, 6, 7];
        let mut cursor = 0;
        for i in 0..(CHUNK_SIZE * 3) {
            bl.append(&data);
            while cursor < (i + 1) * data.len() {
                let chunk = bl.get_chunk(cursor);
                for j in 0..chunk.len() {
                    assert_eq!(chunk[j], data[(cursor + j) % data.len()]);
                }
                cursor += chunk.len();
            }
        }
    }
}
