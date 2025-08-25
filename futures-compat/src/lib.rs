//! Any interaction between an executor/reactor intended for task::Future
//! with an executor/reactor intended for bcsc::Future is strictly unsound.

use std::{
    pin::Pin,
    ptr::NonNull,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use futures_core::Wake;
use lifetime_guard::{atomic_guard::AtomicValueGuard, guard::ValueGuard};

static EVIL_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_| panic!("wtf"),
    |_| panic!("wtf"),
    |_| panic!("wtf"),
    |_| panic!("wtf"),
);

pub type WakePtr = Option<NonNull<dyn Wake>>;

/// Coerces a pinned `ValueGuard` reference to a `Waker` for use in
/// `core::future::Future`
///
/// Any usage or storage of the resulting `Waker` is undefined behavior.
pub unsafe fn guard_to_waker(guard: Pin<&ValueGuard<WakePtr>>) -> Waker {
    unsafe {
        Waker::from_raw(RawWaker::new(
            guard.get_ref() as *const ValueGuard<WakePtr> as *const (),
            &EVIL_VTABLE,
        ))
    }
}
pub unsafe fn atomic_guard_to_waker(
    guard: Pin<&AtomicValueGuard<WakePtr>>,
) -> Waker {
    unsafe {
        Waker::from_raw(RawWaker::new(
            guard.get_ref() as *const AtomicValueGuard<WakePtr> as *const (),
            &EVIL_VTABLE,
        ))
    }
}

/// Coerces a `Waker` into a pinned `AtomicValueGuard` reference.
///
/// This should only be used to undo the work of `guard_to_waker`.
pub unsafe fn waker_to_guard<'a>(
    waker: &Waker,
) -> Pin<&'a ValueGuard<WakePtr>> {
    unsafe {
        Pin::new_unchecked(&*(waker.data() as *const ValueGuard<WakePtr>))
    }
}
pub unsafe fn waker_to_atomic_guard<'a>(
    waker: &Waker,
) -> Pin<&'a AtomicValueGuard<WakePtr>> {
    unsafe {
        Pin::new_unchecked(&*(waker.data() as *const AtomicValueGuard<WakePtr>))
    }
}

// TODO should probably return impl futures_core::Future, same for next fn
pub unsafe fn std_future_to_bespoke<F: core::future::Future>(
    future: F,
) -> NormalFutureWrapper<F> {
    NormalFutureWrapper(future)
}

pub unsafe fn bespoke_future_to_std<F: futures_core::Future>(
    future: F,
) -> BespokeFutureWrapper<F> {
    BespokeFutureWrapper(future)
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

/// wraps custom `bcsc::Future` in impl of `core::future::Future`
#[repr(transparent)]
pub struct BespokeFutureWrapper<F: futures_core::Future>(F);

impl<F: futures_core::Future> core::future::Future for BespokeFutureWrapper<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { self.map_unchecked_mut(|this| &mut this.0).poll(cx) }
    }
}
