//! Redefinitions of task::Future to be incompatible with them

use std::{
    ops::{self, DerefMut},
    pin::Pin,
    task::Poll,
};

/// A future represents an asynchronous computation obtained by use of `async`.
///
/// This future assumes a nonstandard Context, which is incompatible with
/// executors or reactors made for `core::future::Future`. In the interest of
/// safety, it has a dedicated type.
///
/// A future is a value that might not have finished computing yet. This kind of
/// "asynchronous value" makes it possible for a thread to continue doing useful
/// work while it waits for the value to become available.
///
/// # The `poll` method
///
/// The core method of future, `poll`, *attempts* to resolve the future into a
/// final value. This method does not block if the value is not ready. Instead,
/// the current task is scheduled to be woken up when it's possible to make
/// further progress by `poll`ing again. The `context` passed to the `poll`
/// method can provide a [`Waker`], which is a handle for waking up the current
/// task.
///
/// When using a future, you generally won't call `poll` directly, but instead
/// `.await` the value.
///
/// [`Waker`]: crate::task::Waker
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a `bcsc::Future`",
    message = "`{Self}` is not a `bcsc::Future`",
    note = "If you are trying to await a `core::future::Future` from within a `bcsc::Future`, note that the systems are incompatible."
)]
pub trait Future<Waker> {
    /// The type of value produced on completion.
    type Output;

    /// Attempts to resolve the future to a final value, registering
    /// the current task for wakeup if the value is not yet available.
    ///
    /// # Return value
    ///
    /// This function returns:
    ///
    /// - [`Poll::Pending`] if the future is not ready yet
    /// - [`Poll::Ready(val)`] with the result `val` of this future if it
    ///   finished successfully.
    ///
    /// Once a future has finished, clients should not `poll` it again.
    ///
    /// When a future is not ready yet, `poll` returns `Poll::Pending` and
    /// stores a clone of the [`Waker`] copied from the current [`Context`].
    /// This [`Waker`] is then woken once the future can make progress.
    /// For example, a future waiting for a socket to become
    /// readable would call `.clone()` on the [`Waker`] and store it.
    /// When a signal arrives elsewhere indicating that the socket is readable,
    /// [`Waker::wake`] is called and the socket future's task is awoken.
    /// Once a task has been woken up, it should attempt to `poll` the future
    /// again, which may or may not produce a final value.
    ///
    /// Note that on multiple calls to `poll`, only the [`Waker`] from the
    /// [`Context`] passed to the most recent call should be scheduled to
    /// receive a wakeup.
    ///
    /// # Runtime characteristics
    ///
    /// Futures alone are *inert*; they must be *actively* `poll`ed to make
    /// progress, meaning that each time the current task is woken up, it should
    /// actively re-`poll` pending futures that it still has an interest in.
    ///
    /// The `poll` function is not called repeatedly in a tight loop -- instead,
    /// it should only be called when the future indicates that it is ready to
    /// make progress (by calling `wake()`). If you're familiar with the
    /// `poll(2)` or `select(2)` syscalls on Unix it's worth noting that futures
    /// typically do *not* suffer the same problems of "all wakeups must poll
    /// all events"; they are more like `epoll(4)`.
    ///
    /// An implementation of `poll` should strive to return quickly, and should
    /// not block. Returning quickly prevents unnecessarily clogging up
    /// threads or event loops. If it is known ahead of time that a call to
    /// `poll` may end up taking a while, the work should be offloaded to a
    /// thread pool (or something similar) to ensure that `poll` can return
    /// quickly.
    ///
    /// # Panics
    ///
    /// Once a future has completed (returned `Ready` from `poll`), calling its
    /// `poll` method again may panic, block forever, or cause other kinds of
    /// problems; the `Future` trait places no requirements on the effects of
    /// such a call. However, as the `poll` method is not marked `unsafe`,
    /// Rust's usual rules apply: calls must never cause undefined behavior
    /// (memory corruption, incorrect use of `unsafe` functions, or the like),
    /// regardless of the future's state.
    ///
    /// [`Poll::Ready(val)`]: Poll::Ready
    /// [`Waker`]: crate::task::Waker
    /// [`Waker::wake`]: crate::task::Waker::wake
    fn poll(self: Pin<&mut Self>, waker: Pin<&Waker>) -> Poll<Self::Output>;
}

impl<Waker, F: ?Sized + Future<Waker> + Unpin> Future<Waker> for &mut F {
    type Output = F::Output;

    fn poll(
        mut self: Pin<&mut Self>,
        waker: Pin<&Waker>,
    ) -> Poll<Self::Output> {
        F::poll(Pin::new(&mut **self), waker)
    }
}

impl<Waker, P> Future<Waker> for Pin<P>
where
    P: ops::DerefMut<Target: Future<Waker>>,
{
    type Output = <<P as ops::Deref>::Target as Future<Waker>>::Output;

    fn poll(self: Pin<&mut Self>, waker: Pin<&Waker>) -> Poll<Self::Output> {
        <P::Target as Future<Waker>>::poll(self.as_deref_mut(), waker)
    }
}

/// A future which tracks whether or not the underlying future
/// should no longer be polled.
///
/// `is_terminated` will return `true` if a future should no longer be polled.
/// Usually, this state occurs after `poll` (or `try_poll`) returned
/// `Poll::Ready`. However, `is_terminated` may also return `true` if a future
/// has become inactive and can no longer make progress and should be ignored
/// or dropped rather than being `poll`ed again.
pub trait FusedFuture<Waker>: Future<Waker> {
    /// Returns `true` if the underlying future should no longer be polled.
    fn is_terminated(&self) -> bool;
}

impl<Waker, F: FusedFuture<Waker> + ?Sized + Unpin> FusedFuture<Waker>
    for &mut F
{
    fn is_terminated(&self) -> bool {
        <F as FusedFuture<Waker>>::is_terminated(&**self)
    }
}

impl<Waker, P> FusedFuture<Waker> for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: FusedFuture<Waker>,
{
    fn is_terminated(&self) -> bool {
        <P::Target as FusedFuture<Waker>>::is_terminated(&**self)
    }
}

/// temporary trait until Fn::call is stabilized
pub trait Wake {
    fn wake(&self);
}
