//! Any interaction between an executor/reactor intended for task::Future
//! with an executor/reactor intended for bcsc::Future is strictly unsound.

use std::{
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use lifetime_guard::atomic_guard::AtomicValueGuard;

static EVIL_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_| panic!("wtf"),
    |_| panic!("wtf"),
    |_| panic!("wtf"),
    |_| panic!("wtf"),
);

/// Coerces a pinned `AtomicValueGuard` reference to a `Waker` for use in
/// `core::future::Future`
///
/// Any usage or storage of the resulting `Waker` is undefined behavior.
pub unsafe fn guard_to_waker(guard: Pin<&AtomicValueGuard<fn()>>) -> Waker {
    unsafe {
        Waker::from_raw(RawWaker::new(
            guard.get_ref() as *const AtomicValueGuard<fn()> as *const (),
            &EVIL_VTABLE,
        ))
    }
}

/// Coerces a `Waker` into a pinned `AtomicValueGuard` reference.
///
/// This should only be used to undo the work of `guard_to_waker`.
pub unsafe fn waker_to_guard<'a>(
    waker: Waker,
) -> Pin<&'a AtomicValueGuard<fn()>> {
    unsafe {
        Pin::new_unchecked(&*(waker.data() as *const AtomicValueGuard<fn()>))
    }
}

/// wraps `core::future::Future` in impl of `bcsc:Future`
#[repr(transparent)]
pub struct NormalFutureWrapper<F: core::future::Future>(F);

impl<F: core::future::Future> futures_core::Future for NormalFutureWrapper<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { self.map_unchecked_mut(|this| &mut this.0).poll(cx) }
    }
}

impl<F: core::future::Future> NormalFutureWrapper<F> {
    pub unsafe fn from_std_future(future: F) -> Self {
        Self(future)
    }
}

/// wraps custom `bcsc::Future` in impl of `core::future::Future`
#[repr(transparent)]
pub struct CustomFutureWrapper<F: futures_core::Future>(F);

impl<F: futures_core::Future> core::future::Future for CustomFutureWrapper<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { self.map_unchecked_mut(|this| &mut this.0).poll(cx) }
    }
}

impl<F: futures_core::Future> CustomFutureWrapper<F> {
    pub unsafe fn from_custom_future(future: F) -> Self {
        Self(future)
    }
}
