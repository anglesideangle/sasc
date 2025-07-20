//! Definition of the MaybeDone combinator

use futures_core::{ScopedFuture, Wake};

use super::assert_future;
use std::{
    cell::UnsafeCell,
    task::{Poll, ready},
};

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct MaybeDone<'scope, Fut: ScopedFuture<'scope>> {
    state: UnsafeCell<MaybeDoneState<'scope, Fut>>,
}

/// A future that may have completed.
///
/// This is created by the [`maybe_done()`] function.
#[derive(Debug)]
pub enum MaybeDoneState<'scope, Fut: ScopedFuture<'scope>> {
    /// A not-yet-completed future
    Future(Fut),
    /// The output of the completed future
    Done(Fut::Output),
    /// The empty variant after the result of a [`MaybeDone`] has been
    /// taken using the [`take_output`](MaybeDone::take_output) method.
    Gone,
}

/// Wraps a future into a `MaybeDone`
///
///
pub fn maybe_done<'scope, Fut: ScopedFuture<'scope>>(
    future: Fut,
) -> MaybeDone<'scope, Fut> {
    assert_future::<(), _>(MaybeDone {
        state: MaybeDoneState::Future(future).into(),
    })
}

impl<'scope, Fut: ScopedFuture<'scope>> MaybeDone<'scope, Fut> {
    /// Attempt to take the output of a `MaybeDone` without driving it
    /// towards completion.
    #[inline]
    pub fn take_output(&self) -> Option<Fut::Output> {
        match unsafe { &*self.state.get() } {
            MaybeDoneState::Done(_) => {}
            MaybeDoneState::Future(_) | MaybeDoneState::Gone => return None,
        }
        match unsafe { self.state.get().replace(MaybeDoneState::Gone) } {
            MaybeDoneState::Done(output) => Some(output),
            _ => unreachable!(),
        }
    }

    /// Returns an immutable reference to the internal state of this future
    ///
    /// # Safety
    /// You must not hold this reference past any use of any other methods of this struct
    pub unsafe fn get_state(&self) -> &MaybeDoneState<'scope, Fut> {
        unsafe { &*self.state.get() }
    }

    pub fn is_done(&self) -> bool {
        match unsafe { &*self.state.get() } {
            MaybeDoneState::Future(_) => false,
            MaybeDoneState::Done(_) | MaybeDoneState::Gone => true,
        }
    }
}

impl<'scope, Fut: ScopedFuture<'scope>> ScopedFuture<'scope>
    for MaybeDone<'scope, Fut>
{
    type Output = ();

    fn poll(&'scope self, cx: &'scope dyn Wake<'scope>) -> Poll<Self::Output> {
        match unsafe { &*self.state.get() } {
            MaybeDoneState::Future(f) => {
                let res = ready!(f.poll(cx));
                // this is fine because no immutable references currently exist
                unsafe { self.state.get().replace(MaybeDoneState::Done(res)) };
            }
            MaybeDoneState::Done(_) => {}
            MaybeDoneState::Gone => {
                panic!("MaybeDone polled after value taken")
            }
        }
        Poll::Ready(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use crate::poll_fn;

    use crate::noop_wake;

    use super::*;

    #[test]
    fn immediate_return() {
        let immediate = poll_fn(|_| Poll::Ready(1));
        let future = maybe_done(immediate);
        let wake = noop_wake();
        match unsafe { future.get_state() } {
            MaybeDoneState::Future(_) => {}
            MaybeDoneState::Done(_) | MaybeDoneState::Gone => {
                panic!("should be MaybeDoneState::Future")
            }
        }
        assert_eq!(future.poll(&wake), Poll::Ready(()));
        match unsafe { future.get_state() } {
            MaybeDoneState::Done(_) => {}
            MaybeDoneState::Future(_) | MaybeDoneState::Gone => {
                panic!("should be MaybeDoneState::Done")
            }
        }
        assert_eq!(future.take_output(), Some(1));
        assert_eq!(future.take_output(), None);
    }

    #[test]
    fn normal() {
        let x = Cell::new(0);
        let poll = poll_fn(|wake| {
            wake.wake();
            x.set(x.get() + 1);
            if x.get() == 4 {
                Poll::Ready(x.get())
            } else {
                Poll::Pending
            }
        });
        let future = maybe_done(poll);
        let noop = noop_wake();
        for _ in 0..3 {
            assert_eq!(future.poll(&noop), Poll::Pending);
            assert_eq!(future.take_output(), None);
        }
        assert_eq!(future.poll(&noop), Poll::Ready(()));
        assert_eq!(future.poll(&noop), Poll::Ready(()));
        assert_eq!(future.take_output(), Some(4));
    }
}
