use std::{cell::RefCell, io::Read, rc::Rc};

// Convenience traits for working with non-contiguous buffers and reading from things behind a
// RefCell.

pub trait ChunkedBuf {
    // Get a contiguous chunk starting from start. Panics if start >= len.
    fn get_chunk(&self, start: usize) -> &[u8];
    fn len(&self) -> usize;
}

pub trait ChunkedBufExt: ChunkedBuf {
    fn common_prefix_from_to(&self, other: &impl ChunkedBuf, start: usize, end: Option<usize>) -> usize;
    fn common_prefix_from(&self, other: &impl ChunkedBuf, start: usize) -> usize;
    fn at(&self, pos: usize) -> u8;
    fn iter_chunks(&self, start: usize, end: usize) -> IterChunks<'_, Self>;
}

impl<T: ChunkedBuf> ChunkedBufExt for T {
    // Compare with another buf from position start. Position end is the first position at which
    // the two streams differ (or end of one of the streams).
    //
    // TODO: this compiles to byte-by-byte comparison loop. Pretty good, but maybe we can do better
    // by comparing words or even SIMD. Profile.
    fn common_prefix_from(&self, other: &impl ChunkedBuf, start: usize) -> usize {
        self.common_prefix_from_to(other, start, None)
    }

    fn common_prefix_from_to(&self, other: &impl ChunkedBuf, start: usize, end: Option<usize>) -> usize {
        let mut at = start;
        let mut max_cmp = std::cmp::min(self.len(), other.len());
        if let Some(e) = end {
            max_cmp = std::cmp::min(max_cmp, e);
        }

        while at < max_cmp {
            let my_chunk = self.get_chunk(at);
            let other_chunk = other.get_chunk(at);
            let eq_len = my_chunk.iter().zip(other_chunk).take_while(|(a, b)| a == b).count();
            at += eq_len;
            if eq_len < std::cmp::min(my_chunk.len(), other_chunk.len()) {
                return at;
            }
        }
        at
    }

    // Can't implement Index for generic trait :(
    fn at(&self, pos: usize) -> u8 {
        self.get_chunk(pos)[0]
    }

    // Iterate over chunks from start to end.
    fn iter_chunks(&self, start: usize, end: usize) -> IterChunks<'_, Self> {
        assert!(start <= end);
        assert!(end <= self.len());
        IterChunks { b: self, start, end }
    }
}

pub struct IterChunks<'a, T: ChunkedBuf + ?Sized> {
    b: &'a T,
    start: usize,
    end: usize,
}

impl<'a, T: ChunkedBuf> Iterator for IterChunks<'a, T> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.start == self.end {
            return None;
        }
        let mut chunk = self.b.get_chunk(self.start);
        if self.start + chunk.len() > self.end {
            chunk = &chunk[..self.end - self.start];
        }
        self.start += chunk.len();
        Some(chunk)
    }
}

/* Read that doesn't need to keep a reference to self. Used to implement Read for a value behind a
 * RefCell.
 */
pub trait ReadAt {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize>;
}

impl<T: ChunkedBuf> ReadAt for T {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        if start >= self.len() {
            return Ok(0);
        }
        self.get_chunk(start).read(buf)
    }
}

impl<T: ReadAt> ReadAt for Rc<RefCell<T>> {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        self.borrow().read_at(start, buf)
    }
}

pub struct ReadAtCursor<'a, T: ReadAt + ?Sized> {
    src: &'a T,
    start: usize,
}

impl<'a, T: ReadAt + ?Sized> ReadAtCursor<'a, T> {
    pub fn position(&self) -> usize {
        self.start
    }
}

impl<'a, T: ReadAt + ?Sized> Read for ReadAtCursor<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let res = self.src.read_at(self.start, buf)?;
        self.start += res;
        Ok(res)
    }
}

pub trait ReadAtExt: ReadAt {
    fn reader_from(&self, start: usize) -> ReadAtCursor<'_, Self>;
    fn reader(&self) -> ReadAtCursor<'_, Self>;
}

impl<T: ReadAt> ReadAtExt for T {
    fn reader_from(&self, start: usize) -> ReadAtCursor<'_, Self> {
        ReadAtCursor { src: self, start }
    }
    fn reader(&self) -> ReadAtCursor<'_, Self> {
        self.reader_from(0)
    }
}
