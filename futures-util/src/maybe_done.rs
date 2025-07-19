//! Definition of the MaybeDone combinator

use futures_core::{ScopedFuture, Wake};

use super::assert_future;
use core::mem;
use core::pin::Pin;
use std::{
    cell::{Cell, RefCell, UnsafeCell},
    ptr,
    task::{Poll, ready},
};

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
/// # Examples
///
/// ```
/// # futures::executor::block_on(async {
/// use core::pin::pin;
///
/// use futures::future;
///
/// let future = future::maybe_done(async { 5 });
/// let mut future = pin!(future);
/// assert_eq!(future.as_mut().take_output(), None);
/// let () = future.as_mut().await;
/// assert_eq!(future.as_mut().take_output(), Some(5));
/// assert_eq!(future.as_mut().take_output(), None);
/// # });
/// ```
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
