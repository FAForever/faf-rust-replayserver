use std::cell::RefCell;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/* Awaitable progress tracker. Some tasks can wait for the tracker to reach some progress level,
 * others can set progress.
 */

pub trait ProgressKey: Copy + Ord {
    fn bottom() -> Self;
    fn top() -> Self;
}

struct Inner<T: ProgressKey> {
    waiters: BinaryHeap<Reverse<WakerToken<T>>>,
    pub position: T,
}

struct WakerToken<T: ProgressKey> {
    pos: T,
    waker: Waker,
}

pub struct WaitProgressFuture<T: ProgressKey> {
    inner: Rc<RefCell<Inner<T>>>,
    until: T,
}

pub struct ProgressTracker<T: ProgressKey> {
    inner: Rc<RefCell<Inner<T>>>,
}

impl<T: ProgressKey> PartialEq for WakerToken<T> {
    fn eq(&self, other: &Self) -> bool {
        return self.pos.eq(&other.pos);
    }
}
impl<T: ProgressKey> Eq for WakerToken<T> {}
impl<T: ProgressKey> PartialOrd for WakerToken<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        return self.pos.partial_cmp(&other.pos);
    }
}
impl<T: ProgressKey> Ord for WakerToken<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        return self.pos.cmp(&other.pos);
    }
}

impl<T: ProgressKey> Inner<T> {
    fn new() -> Self {
        Self {
            waiters: BinaryHeap::new(),
            position: T::bottom(),
        }
    }

    pub fn was_reached(&self, pos: T) -> bool {
        pos <= self.position
    }

    pub fn insert(&mut self, token: WakerToken<T>) {
        self.waiters.push(Reverse(token));
    }

    pub fn advance(&mut self, next: T) {
        debug_assert!(next >= self.position);
        self.position = next;
        self.check_tokens()
    }

    pub fn check_tokens(&mut self) {
        loop {
            match self.waiters.peek() {
                None => return,
                Some(rw_ref) => {
                    let Reverse(w_ref) = rw_ref;
                    if !self.was_reached(w_ref.pos) {
                        return;
                    }
                    let Reverse(w) = self.waiters.pop().unwrap();
                    let WakerToken { waker, .. } = w;
                    waker.wake();
                }
            }
        }
    }
}

impl<T: ProgressKey> WaitProgressFuture<T> {
    fn new(inner: Rc<RefCell<Inner<T>>>, until: T) -> Self {
        Self { inner, until }
    }
}

impl<T: ProgressKey> Future for WaitProgressFuture<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut inner = self.inner.borrow_mut();
        if inner.was_reached(self.until) {
            return Poll::Ready(inner.position);
        }
        let token = WakerToken {
            pos: self.until,
            waker: cx.waker().clone(),
        };
        inner.insert(token);
        Poll::Pending
    }
}

impl<T: ProgressKey> ProgressTracker<T> {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(Inner::new())),
        }
    }

    // Returns current progress (potentially further than waited for).
    pub fn wait(&self, until: T) -> WaitProgressFuture<T> {
        WaitProgressFuture::new(self.inner.clone(), until)
    }

    pub fn advance(&mut self, next: T) {
        self.inner.borrow_mut().advance(next)
    }

    pub fn position(&self) -> T {
        self.inner.borrow().position
    }
}

impl<T: ProgressKey> Drop for ProgressTracker<T> {
    /* Sanity check. IMO better to require caller to explicitly reach progress end.*/
    fn drop(&mut self) {
        if self.position() != T::top() {
            // TODO do not panic, as this causes aborts when other tests panic.
            // What to do with this then? Log error and hope for the best?
            // panic!("Progress tracker was dropped without reaching the end");
        }
    }
}
