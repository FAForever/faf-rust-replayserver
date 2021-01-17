use std::io::Write;
use super::buf_traits::DiscontiguousBuf;

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
            len: 0
        }
    }
    fn chunk(&self, start: usize) -> (usize, usize, usize) {
        assert!(start < self.len());
        let chunk_idx = start / CHUNK_SIZE;
        let offset = start % CHUNK_SIZE;
        let end = std::cmp::min(CHUNK_SIZE, self.len - (chunk_idx * CHUNK_SIZE));
        (chunk_idx, offset, end)
    }
}

impl DiscontiguousBuf for BufList {
    fn get_chunk(&self, start: usize) -> &[u8] {
        let (i, o, e) = self.chunk(start);
        &self.chunks[i][o..e]
    }

    fn len(&self) -> usize {
        self.len
    }

    fn get_mut_chunk(&mut self, start: usize) -> &mut [u8] {
        let (i, o, e) = self.chunk(start);
        &mut self.chunks[i][o..e]
    }
}

impl Write for BufList {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.len % CHUNK_SIZE == 0 {
            self.chunks.push(Box::new([0; CHUNK_SIZE]));
        }
        let written = self.get_mut_chunk(self.len).write(buf)?;
        self.len += written;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
