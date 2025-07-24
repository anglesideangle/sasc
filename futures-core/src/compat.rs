//! Any interaction between the real Future ecosystem and ScopedFuture
//! ecosystem is strictly unsound
//!
//! ScopedFutures cannot poll Futures because they can't guarantee they
//! will outlive *const () ptrs they supply to the futures, leading to
//! dangling pointers if the futures register the waker with something that
//! assumes assumes it is valid for 'static and then the ScopedFuture goes
//! out of scope
//!
//! Futures cannot poll ScopedFutures because they cannot guarantee their
//! Waker will be valid for 'scope (due to lack of real borrowing), leading to
//! unsoundness if a ScopedFuture internally registers the waker with something
//! that expects it to live for 'scope, and then the ScopedFutureWrapper is
//! dropped

use crate::{ScopedFuture, Wake};
use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    mem,
    pin::Pin,
    task::{Context, Poll, Waker},
};

/// RawWaker: fat ptr (*const (), &'static RawWakerVTable)
/// &'scope dyn Wake fat ptr: (&'scope (), &'scope WakeVTable)
///
/// can transmute between them, but the waker will be completely invalid!

/// wraps an internal ScopedFuture, implements Future
pub struct ScopedFutureWrapper<'scope, F: ScopedFuture<'scope> + 'scope> {
    inner: UnsafeCell<F>,
    marker: PhantomData<&'scope ()>,
}

impl<'scope, F: ScopedFuture<'scope> + 'scope> Future
    for ScopedFutureWrapper<'scope, F>
{
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        // # Safety
        //
        // Transmutes `Waker` into `&'scope dyn Wake`.
        // This is possible because Waker (internally just RawWaker) contains
        // (*const (), &'static RawWakerVTable), and the fat ptr `&dyn Wake`
        // internally is (*const (), *const WakeVTable).
        //
        // For this to be sound, the input waker from `cx` must be an invalid
        // waker (using the waker as intended would be UB) that has the form of
        // a `&dyn Wake` fat ptr, as generated in `UnscopedFutureWrapper`.
        //
        // This is only sound because it is paired with the transmute in
        // `UnscopedFutureWrapper`
        //
        // This conversion is necessary to piggyback off rustc's expansion
        // of `async` blocks into state machines implementing `Future`.
        //
        // The unpinning is safe because inner (a `ScopedFuture`) cannot be
        // moved after a self reference is established on its first `poll`.
        //
        // The use of the `UnsafeCell` is sound and necessary to get around
        // the afformentioned immutable self reference (since `Future::poll`)
        // requires a `&mut Self`. It is sound because we never take a
        // `&mut self.inner`.
        unsafe {
            let this = self.get_unchecked_mut();
            let wake: &'scope dyn Wake = mem::transmute::<
                Waker,
                &'scope dyn Wake,
            >(cx.waker().to_owned());
            (&*this.inner.get()).poll(wake)
        }
    }
}

impl<'scope, F: ScopedFuture<'scope> + 'scope> ScopedFutureWrapper<'scope, F> {
    pub unsafe fn from_scoped(f: F) -> Self {
        Self {
            inner: f.into(),
            marker: PhantomData,
        }
    }
}

/// wraps an internal Future, implements ScopedFuture
/// this is fundamentally unsafe and relies on the future not registering its waker
/// in any reactor that lives beyond this wrapper, otherwise there will be a dangling pointer
///
/// it is safe to use only with the #[async_scoped] macro, which guarantees that, internally, every futures is a ScopedFutureWrapper
pub struct UnscopedFutureWrapper<'scope, F: Future> {
    inner: UnsafeCell<F>,
    marker: PhantomData<&'scope ()>,
}

impl<'scope, F: Future> ScopedFuture<'scope>
    for UnscopedFutureWrapper<'scope, F>
{
    type Output = F::Output;

    fn poll(
        self: &'scope Self,
        wake: &'scope dyn Wake<'scope>,
    ) -> Poll<Self::Output> {
        // # Safety
        //
        // Transmutes `&'scope dyn Wake` into a Waker.
        // This is possible because Waker (internally just RawWaker) contains
        // (*const (), &'static RawWakerVTable), and the fat ptr `&dyn Wake`
        // internally is (*const (), *const WakeVTable).
        //
        // Using the resulting Waker is UB, which is why UnscopedFutureWrapper
        // can only be used in pair with ScopedFutureWrapper, which transmutes
        // the invalid `Waker` back to a `&dyn Wake`.
        //
        // This conversion is necessary to piggyback off rustc's expansion
        // of `async` blocks into state machines implementing `Future`.
        let waker: Waker =
            unsafe { mem::transmute::<&'scope dyn Wake<'scope>, Waker>(wake) };
        // # Safety
        //
        // Once any ScopedFuture is first polled and stores a waker, it becomes
        // immutable and immovable because it has an immutable self reference.
        //
        // The Pin::new_unchecked is necessary to be compatible with
        // `task::Future`
        let pinned_future =
            unsafe { Pin::new_unchecked(&mut *self.inner.get()) };

        pinned_future.poll(&mut Context::from_waker(&waker))
    }
}

impl<'scope, F: Future> UnscopedFutureWrapper<'scope, F> {
    pub unsafe fn from_future(f: F) -> Self {
        Self {
            inner: f.into(),
            marker: PhantomData,
        }
    }
}
