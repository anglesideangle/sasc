use core::fmt;
use std::task::Poll;

use futures_core::{ScopedFuture, Wake};

use crate::assert_future;

/// Future for the [`poll_fn`] function.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct PollFn<F> {
    f: F,
}

/// Creates a new future wrapping around a function returning [`Poll`].
///
/// Polling the returned future delegates to the wrapped function.
///
/// # Examples
///
/// ```
/// # futures::executor::block_on(async {
/// use futures::future::poll_fn;
/// use futures::task::{Context, Poll};
///
/// fn read_line(_cx: &mut Context<'_>) -> Poll<String> {
///     Poll::Ready("Hello, World!".into())
/// }
///
/// let read_future = poll_fn(read_line);
/// assert_eq!(read_future.await, "Hello, World!".to_owned());
/// # });
/// ```
pub fn poll_fn<'scope, T, F>(f: F) -> PollFn<F>
where
    F: Fn(&'scope dyn Wake) -> Poll<T>,
{
    assert_future::<T, _>(PollFn { f })
}

impl<F> fmt::Debug for PollFn<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollFn").finish()
    }
}

impl<'scope, T, F> ScopedFuture<'scope> for PollFn<F>
where
    F: Fn(&'scope dyn Wake) -> Poll<T>,
{
    type Output = T;

    fn poll(&self, wake: &'scope dyn Wake) -> Poll<T> {
        (&self.f)(wake)
    }
}
