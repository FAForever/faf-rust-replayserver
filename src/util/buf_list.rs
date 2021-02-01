use super::buf_traits::{DiscontiguousBuf, DiscontiguousBufExt};
use std::io::Write;

const CHUNK_SIZE: usize = 4096;

// I guess I could use vec<u8> instead. But, maybe one day I'll implement some memory savings like
// compression with this.
pub struct BufList {
    chunks: Vec<Box<[u8; CHUNK_SIZE]>>,
    len: usize,
}

impl BufList {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

impl DiscontiguousBuf for BufList {
    fn get_chunk(&self, start: usize) -> &[u8] {
        assert!(start < self.len());
        let chunk_idx = start / CHUNK_SIZE;
        let offset = start % CHUNK_SIZE;
        let end = std::cmp::min(CHUNK_SIZE, self.len - (chunk_idx * CHUNK_SIZE));
        &self.chunks[chunk_idx][offset..end]
    }

    fn len(&self) -> usize {
        self.len
    }

    fn append_some(&mut self, buf: &[u8]) -> usize {
        if self.len % CHUNK_SIZE == 0 {
            self.chunks.push(Box::new([0; CHUNK_SIZE]));
        }

        let last_idx = self.chunks.len() - 1;
        let offset = self.len % CHUNK_SIZE;
        let mut last_chunk: &mut [u8] = &mut self.chunks[last_idx][offset..];
        let written = last_chunk.write(buf).unwrap();
        self.len += written;
        written
    }
}

impl Write for BufList {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.append(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
