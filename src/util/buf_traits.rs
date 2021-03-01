use std::{cell::RefCell, io::Read, rc::Rc};

// Buffer made up of smaller contiguous chunks. We use those to discard data more easily.

// We slighly break rules for buffers we can discard from. Trying to access data that was already
// discarded causes a panic.
//
// TODO split into mut / nonmut variants?
pub trait DiscontiguousBuf {
    // Get a contiguous chunk starting from start. Panics if start >= len.
    fn get_chunk(&self, start: usize) -> &[u8];
    fn len(&self) -> usize;
    fn append_some(&mut self, buf: &[u8]) -> usize;
}

pub trait DiscontiguousBufExt {
    fn append(&mut self, buf: &[u8]);
    fn common_prefix_from(&self, other: &impl DiscontiguousBuf, start: usize) -> usize;
    fn at(&self, pos: usize) -> u8;
}

impl<T: DiscontiguousBuf> DiscontiguousBufExt for T {
    fn append(&mut self, mut buf: &[u8]) {
        while buf.len() > 0 {
            let off = self.append_some(buf);
            buf = &buf[off..];
        }
    }
    // Compare with another buf from position start. Position end is the first position at which
    // the two streams differ (or end of one of the streams).
    //
    // TODO: this compiles to byte-by-byte comparison loop. Pretty good, but maybe we can do better
    // by comparing words or even SIMD. Profile.
    fn common_prefix_from(&self, other: &impl DiscontiguousBuf, start: usize) -> usize {
        let mut at = start;
        let max_cmp = std::cmp::min(self.len(), other.len());
        while at < max_cmp {
            let my_chunk = self.get_chunk(at);
            let other_chunk = other.get_chunk(at);
            let eq_len = my_chunk
                .iter()
                .zip(other_chunk)
                .take_while(|(a, b)| a == b)
                .count();
            at += eq_len;
            if eq_len < std::cmp::min(my_chunk.len(), other_chunk.len()) {
                return at;
            }
        }
        at
    }

    /* Can't implement Index for generic trait :( */
    fn at(&self, pos: usize) -> u8 {
        self.get_chunk(pos)[0]
    }
}

/* For merging incoming replays we want a data structure that we can append bytes to front and
 * discard bytes from back whenever we want. We accept responsibility to never read data we already
 * discarded.
 */
pub trait BufWithDiscard {
    /* Discard all data up to 'until'.
     * Can discard beyond data end, in that case new writes will only increase the end value until
     * it reaches discard point.
     * */
    fn discard(&mut self, until: usize);
}

pub trait BufWithDiscardExt: BufWithDiscard {
    fn discard_all(&mut self) {
        self.discard(usize::MAX);
    }
}

/* Read that doesn't need to keep a reference to self. Used to implement Read for a value behind a
 * RefCell.
 */
pub trait ReadAt {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize>;
}

impl<T: DiscontiguousBuf> ReadAt for T {
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
    fn reader_from<'a>(&'a self, start: usize) -> ReadAtCursor<'a, Self>;
    fn reader<'a>(&'a self) -> ReadAtCursor<'a, Self>;
}

impl<T: ReadAt> ReadAtExt for T {
    fn reader_from<'a>(&'a self, start: usize) -> ReadAtCursor<'a, Self> {
        ReadAtCursor { src: self, start }
    }
    fn reader<'a>(&'a self) -> ReadAtCursor<'a, Self> {
        self.reader_from(0)
    }
}
