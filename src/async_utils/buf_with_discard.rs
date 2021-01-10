use std::{cell::RefCell, rc::Rc, io::Read};

/* For merging incoming replays we want a data structure that we can append bytes to and discard
 * bytes from front whenever we want. We accept responsibility to never read data we already
 * discarded.
 */

pub trait BufWithDiscard {
    fn append(&mut self, buf: &[u8]);
    /* Get largest contiguous chunk from start, without unread data. */
    fn get_chunk(&self, start: usize) -> &[u8];
    /* Discard all data up to 'until'.
     * Can discard beyond data end, in that case new writes will only increase the end value until
     * it reaches discard point.
     * */
    fn discard_start(&self) -> usize;
    fn end(&self) -> usize;
    fn discard(&mut self, until: usize);
}

pub trait BufWithDiscardExt: BufWithDiscard {
    fn discard_all(&mut self) {
        self.discard(usize::MAX);
    }
}

/* Some magic below so we can wrap a BufWithDiscard in a reader. In an async context we can't hold
 * a reference to a RefCell across an await, hence below trait.
 */
pub trait ReadAt {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize>;
}

impl<T: BufWithDiscard> ReadAt for T {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.discard_start() > start {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Data attempted to read was already discarded!"))
        }
        let mut chunk = self.get_chunk(start);
        chunk.read(buf)
    }
}

impl<T: ReadAt> ReadAt for Rc<RefCell<T>> {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        self.borrow().read_at(start, buf)
    }
}

pub struct BufWithDiscardCursor<'a, T: ReadAt + ?Sized> {
    bwd: &'a T,
    start: usize,
}

impl<'a, T: ReadAt + ?Sized> Read for BufWithDiscardCursor<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let res = self.bwd.read_at(self.start, buf)?;
        self.start += res;
        Ok(res)
    }
}

pub trait ReadAtExt: ReadAt {
    fn reader_from<'a>(&'a self, start: usize) -> BufWithDiscardCursor<'a, Self> {
        BufWithDiscardCursor {bwd: self, start}
    }
    fn reader<'a>(&'a self) -> BufWithDiscardCursor<'a, Self> {
        self.reader_from(0)
    }
}
