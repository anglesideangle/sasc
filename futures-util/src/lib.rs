use std::{pin::Pin, ptr::NonNull, task::Poll};

use futures_core::{Future, Wake};
use lifetime_guard::{atomic_guard::AtomicValueGuard, guard::ValueGuard};

pub mod block_on;
pub mod maybe_done;

pub type WakePtr = Option<NonNull<dyn Wake>>;
pub type LocalWaker = ValueGuard<WakePtr>;
pub type AtomicWaker = AtomicValueGuard<WakePtr>;

pub(crate) fn assert_future<T, F>(future: F) -> F
where
    F: Future<LocalWaker, Output = T>,
{
    future
}

pub struct PollFn<F, T>(F)
where
    F: FnMut(&LocalWaker) -> Poll<T>;

impl<F, T> futures_core::Future<LocalWaker> for PollFn<F, T>
where
    F: FnMut(&LocalWaker) -> Poll<T>,
{
    type Output = T;

    fn poll(
        self: Pin<&mut Self>,
        waker: Pin<&LocalWaker>,
    ) -> Poll<Self::Output> {
        (unsafe { &mut self.get_unchecked_mut().0 })(&waker)
    }
}

pub fn poll_fn<F, T>(f: F) -> impl futures_core::Future<LocalWaker, Output = T>
where
    F: FnMut(&LocalWaker) -> Poll<T>,
{
    PollFn(f)
}

pub struct DummyWaker;

impl Wake for DummyWaker {
    fn wake(&self) {
        dbg!("awake!");
    }
}

pub fn dummy_guard() -> ValueGuard<WakePtr> {
    ValueGuard::new(NonNull::new(&mut DummyWaker as *mut dyn Wake))
}
