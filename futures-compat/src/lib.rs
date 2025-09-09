//! Any interaction between an executor/reactor intended for task::Future
//! with an executor/reactor intended for bcsc::Future is strictly unsound.

use std::{
    hint::unreachable_unchecked,
    mem::ManuallyDrop,
    pin::Pin,
    ptr::NonNull,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use futures_core::Wake;
use lifetime_guard::{atomic_guard::AtomicValueGuard, guard::ValueGuard};

pub type WakePtr = Option<NonNull<dyn Wake>>;
pub type LocalWaker = ValueGuard<WakePtr>;
pub type AtomicWaker = AtomicValueGuard<WakePtr>;

static EVIL_VTABLE: RawWakerVTable = unsafe {
    RawWakerVTable::new(
        |_| unreachable_unchecked(),
        |_| unreachable_unchecked(),
        |_| unreachable_unchecked(),
        |_| unreachable_unchecked(),
    )
};

/// Coerces a pinned `ValueGuard` reference to a `Waker` for use in
/// `core::future::Future`
///
/// Any usage or storage of the resulting `Waker` is undefined behavior.
pub unsafe fn guard_to_waker(guard: Pin<&LocalWaker>) -> ManuallyDrop<Waker> {
    ManuallyDrop::new(unsafe {
        Waker::from_raw(RawWaker::new(
            guard.get_ref() as *const ValueGuard<WakePtr> as *const (),
            &EVIL_VTABLE,
        ))
    })
}

pub unsafe fn atomic_guard_to_waker(
    guard: Pin<&AtomicWaker>,
) -> ManuallyDrop<Waker> {
    ManuallyDrop::new(unsafe {
        Waker::from_raw(RawWaker::new(
            guard.get_ref() as *const AtomicValueGuard<WakePtr> as *const (),
            &EVIL_VTABLE,
        ))
    })
}

/// Coerces a `Waker` into a pinned `AtomicValueGuard` reference.
///
/// This should only be used to undo the work of `guard_to_waker`.
pub unsafe fn waker_to_guard<'a>(waker: &Waker) -> Pin<&LocalWaker> {
    unsafe {
        Pin::new_unchecked(&*(waker.data() as *const ValueGuard<WakePtr>))
    }
}

pub unsafe fn waker_to_atomic_guard<'a>(waker: &Waker) -> Pin<&AtomicWaker> {
    unsafe {
        Pin::new_unchecked(&*(waker.data() as *const AtomicValueGuard<WakePtr>))
    }
}

pub unsafe fn std_future_to_bespoke<F: core::future::Future>(
    future: F,
) -> impl futures_core::Future<LocalWaker, Output = F::Output> {
    NormalFutureWrapper(future)
}

pub unsafe fn bespoke_future_to_std<F: futures_core::Future<LocalWaker>>(
    future: F,
) -> impl core::future::Future<Output = F::Output> {
    BespokeFutureWrapper(future)
}

/// wraps `core::future::Future` in impl of `bcsc:Future`
#[repr(transparent)]
pub struct NormalFutureWrapper<F: core::future::Future>(F);

impl<F: core::future::Future> futures_core::Future<LocalWaker>
    for NormalFutureWrapper<F>
{
    type Output = F::Output;

    fn poll(
        self: Pin<&mut Self>,
        waker: Pin<&LocalWaker>,
    ) -> Poll<Self::Output> {
        unsafe {
            self.map_unchecked_mut(|this| &mut this.0)
                .poll(&mut Context::from_waker(&guard_to_waker(waker)))
        }
    }
}

/// wraps custom `bcsc::Future` in impl of `core::future::Future`
#[repr(transparent)]
pub struct BespokeFutureWrapper<F>(F)
where
    F: futures_core::Future<LocalWaker>;

impl<F> core::future::Future for BespokeFutureWrapper<F>
where
    F: futures_core::Future<LocalWaker>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe {
            self.map_unchecked_mut(|this| &mut this.0)
                .poll(waker_to_guard(cx.waker()))
        }
    }
}

#[cfg(test)]
mod test {
    use std::pin;

    use super::*;
    use futures_core::Wake;

    #[derive(Debug)]
    struct DummyWake;
    impl Wake for DummyWake {
        fn wake(&self) {}
    }

    #[test]
    fn waker_conversion() {
        let dummy = DummyWake;
        let guard = pin::pin!(ValueGuard::new(NonNull::new(
            &dummy as *const dyn Wake as *mut dyn Wake
        )));
        let waker = unsafe { guard_to_waker(guard.as_ref()) };
        let guard = unsafe { waker_to_guard(&waker) };
        assert_eq!(
            guard.get().unwrap().as_ptr() as *const () as usize,
            &dummy as *const _ as *const () as usize
        );
    }
}
