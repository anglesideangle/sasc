use std::{
    array, cell::Cell, marker::PhantomPinned, pin::Pin, ptr::NonNull,
    task::Poll,
};

use futures_compat::{LocalWaker, WakePtr};
use futures_core::Wake;
use lifetime_guard::{guard::RefGuard, guard::ValueGuard};

pub struct WakeArray<const N: usize> {
    parent: RefGuard<WakePtr>,
    children: [ValueGuard<WakePtr>; N],
    stores: [WakeStore; N],
    _marker: PhantomPinned,
}

impl<const N: usize> WakeArray<N> {
    pub fn new() -> Self {
        Self {
            parent: RefGuard::new(),
            children: array::from_fn(|_| ValueGuard::new(None)),
            stores: array::from_fn(|_| WakeStore::new()),
            _marker: PhantomPinned,
        }
    }

    pub fn register_parent(
        self: Pin<&Self>,
        parent: Pin<&ValueGuard<WakePtr>>,
    ) {
        unsafe { Pin::new_unchecked(&self.parent) }.register(parent);
    }

    /// Returns pinned reference to child ValueGuard
    /// returns None if n is not in 0..N
    pub fn child_guard_ptr(
        self: Pin<&Self>,
        index: usize,
    ) -> Option<Pin<&ValueGuard<WakePtr>>> {
        // TODO remove bounds checking, break api when https://github.com/rust-lang/rust/issues/123646
        if index >= N {
            return None;
        }

        let wake_store = unsafe { self.stores.get(index).unwrap_unchecked() };
        wake_store.set_parent(&self.parent);

        let wake_store = unsafe {
            NonNull::new_unchecked(
                wake_store as *const dyn Wake as *mut dyn Wake,
            )
        };

        let child_guard =
            unsafe { self.get_ref().children.get(index).unwrap_unchecked() };
        child_guard.set(Some(wake_store));

        Some(unsafe { Pin::new_unchecked(child_guard) })
    }

    pub fn take_woken(self: Pin<&Self>, index: usize) -> Option<bool> {
        self.stores.get(index).map(|store| store.take_woken())
    }
}

pub struct WakeStore {
    wake_parent: Cell<Option<NonNull<RefGuard<WakePtr>>>>,
    activated: Cell<bool>,
}

impl WakeStore {
    pub fn new() -> Self {
        Self {
            wake_parent: Cell::new(None),
            activated: Cell::new(true),
        }
    }

    pub fn set_parent(&self, parent: &RefGuard<WakePtr>) {
        self.wake_parent.set(Some(parent.into()));
    }

    pub fn take_woken(&self) -> bool {
        self.activated.replace(false)
    }
}

impl Wake for WakeStore {
    fn wake(&self) {
        dbg!("awake?");
        self.activated.set(true);
        if let Some(parent) = self
            .wake_parent
            .get()
            .map(|guard_ptr| unsafe { &*guard_ptr.as_ptr() })
            .and_then(|guard| guard.get())
            .flatten()
        {
            unsafe { &*parent.as_ptr() }.wake();
        }
    }
}

pub fn local_wake(guard: &LocalWaker) {
    if let Some(wake) = guard.get() {
        unsafe { (*wake.as_ptr()).wake() }
    }
}

// pub unsafe fn wake_bespoke_waker(waker: &std::task::Waker) {
//     unsafe {
//         let guard = futures_compat::waker_to_guard(waker);
//         if let Some(wake) = guard.get() {
//             (*wake.as_ptr()).wake();
//         }
//     }
// }

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
