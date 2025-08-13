/// Unsound Drop based guard API
use std::{
    cell::Cell,
    marker::PhantomPinned,
    pin::Pin,
    ptr::{self},
};

pub struct ValueGuard<T> {
    data: T,
    ref_guard: Cell<*const RefGuard<T>>,
    _marker: PhantomPinned,
}

impl<T> ValueGuard<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            ref_guard: Cell::new(ptr::null()),
            _marker: PhantomPinned,
        }
    }

    pub fn registration<'a>(self: Pin<&'a Self>) -> GuardRegistration<'a, T> {
        GuardRegistration { value_guard: self }
    }

    pub fn set(&mut self, value: T) {
        self.data = value;
    }
}

impl<T: Copy> ValueGuard<T> {
    pub fn get(&self) -> T {
        self.data
    }
}

impl<T> Drop for ValueGuard<T> {
    fn drop(&mut self) {
        unsafe {
            self.ref_guard
                .get()
                .as_ref()
                .inspect(|guard| guard.value_guard.set(ptr::null()))
        };
    }
}

pub struct RefGuard<T> {
    value_guard: Cell<*const ValueGuard<T>>,
    _marker: PhantomPinned,
}

impl<T> RefGuard<T> {
    pub fn new() -> Self {
        Self {
            value_guard: Cell::new(ptr::null()),
            _marker: PhantomPinned,
        }
    }
}

impl<T: Copy> RefGuard<T> {
    pub fn get(&self) -> Option<T> {
        unsafe { self.value_guard.get().as_ref().map(|ptr| ptr.data) }
    }
}

impl<T> Drop for RefGuard<T> {
    fn drop(&mut self) {
        unsafe { self.value_guard.get().as_ref() }
            .inspect(|guard| guard.ref_guard.set(ptr::null()));
    }
}

pub struct GuardRegistration<'a, T> {
    value_guard: Pin<&'a ValueGuard<T>>,
}

impl<'a, T> GuardRegistration<'a, T> {
    pub fn from_guard(value_guard: Pin<&'a ValueGuard<T>>) -> Self {
        Self { value_guard }
    }

    pub fn register(self, slot: Pin<&'a RefGuard<T>>) {
        // register new ptrs
        let old_value_guard = slot
            .value_guard
            .replace(self.value_guard.get_ref() as *const ValueGuard<T>);

        let old_ref_guard = self.value_guard.ref_guard.replace(slot.get_ref());

        // annul old ptrs
        unsafe { old_value_guard.as_ref() }
            .inspect(|guard| guard.ref_guard.set(ptr::null()));
        unsafe { old_ref_guard.as_ref() }
            .inspect(|guard| guard.value_guard.set(ptr::null()));
    }
}

#[cfg(test)]
mod test {
    use std::pin;

    use super::*;

    fn consume<T>(input: T) {}

    #[test]
    fn basic() {
        let weak = RefGuard::new();
        let weak_pinned = pin::pin!(weak);
        {
            let mut strong = ValueGuard::new(2);
            let mut strong_pinned = pin::pin!(strong);
            strong_pinned
                .as_ref()
                .registration()
                .register(weak_pinned.as_ref());

            assert_eq!(strong_pinned.get(), 2);
            assert_eq!(weak_pinned.get(), Some(2));

            unsafe { strong_pinned.as_mut().get_unchecked_mut() }.set(3);
            assert_eq!(strong_pinned.get(), 3);
            assert_eq!(weak_pinned.get(), Some(3));
        }

        assert_eq!(weak_pinned.get(), None);
    }
}
