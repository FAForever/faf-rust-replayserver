use std::io::{Read, Write};

use super::buf_with_discard::ReadAt;

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
    fn chunk_at(&self, start: usize) -> &[u8] {
        let (i, o, e) = self.chunk(start);
        &self.chunks[i][o..e]
    }
    
    fn mut_chunk_at(&mut self, start: usize) -> &mut [u8] {
        let (i, o, e) = self.chunk(start);
        &mut self.chunks[i][o..e]
    }

    fn chunk(&self, start: usize) -> (usize, usize, usize) {
        let chunk_idx = start / CHUNK_SIZE;
        let offset = start % CHUNK_SIZE;
        let end = std::cmp::min(CHUNK_SIZE, self.len - (chunk_idx * CHUNK_SIZE));
        (chunk_idx, offset, end)
    }
}

impl ReadAt for BufList {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        if start >= self.len {
            return Ok(0);
        }
        self.chunk_at(start).read(buf)
    }
}

impl Write for BufList {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.len % CHUNK_SIZE == 0 {
            self.chunks.push(Box::new([0; CHUNK_SIZE]));
        }
        let written = self.mut_chunk_at(self.len).write(buf)?;
        self.len += written;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
