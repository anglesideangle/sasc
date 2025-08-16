//! # Lifetime Guard
//!
//! `lifetime-guard` provides `ValueGuard` and `RefGuard` structs to allow for
//! weak references to interior mutable values, similar to a singular pair of
//! `Rc` and `Weak`, but without heap allocation.
//!
//! ## Example Usage
//!
//! ```rust
//! use std::pin;
//! use lifetime_guard::{ ValueGuard, RefGuard };
//!
//! let weak = pin::pin!(RefGuard::new());
//! {
//!     let strong = pin::pin!(ValueGuard::new(0));
//!     strong.as_ref().registration().register(weak.as_ref());
//!
//!     assert_eq!(strong.get(), 0);
//!     assert_eq!(weak.get(), Some(0));
//!
//!     strong.as_ref().set(1);
//!     assert_eq!(strong.get(), 1);
//!     assert_eq!(weak.get(), Some(1));
//! }
//! assert_eq!(weak.get(), None);
//! ```
//!
//! # Safety
//!
//! You *may not* leak any instance of either `ValueGuard` or `RefGuard` to the
//! stack using `mem::forget()` or any other mechanism that causes thier
//! contents to be overwritten without `Drop::drop()` running.
//! Doing so creates unsoundness that likely will lead to dereferencing a null
//! pointer.
//!
//! Doing so creates unsoundness that likely will lead to dereferencing a null
//! pointer. See the
//! [Forget marker trait](https://github.com/rust-lang/rfcs/pull/3782) rfc for
//! progress on making interfaces that rely on not being leaked sound.
//!
//! Note that it is sound to leak `ValueGuard` and `RefGuard` to the heap using
//! methods including `Box::leak()` because heap allocated data will never be
//! overwritten if it is never freed.

use std::{cell::Cell, marker::PhantomPinned, pin::Pin, ptr::NonNull};

/// Strong guard for granting read access to a single interior mutable value to
/// `RefGuard`.
///
/// A `ValueGuard`:`RefGuard` relationship is exclusive, and behaves similarly
/// to a single `Rc` and `Weak` pair, but notably does not require heap
/// allocation. `ValueGuard::registration` creates a `GuardRegistration`, which
/// provides a movable wrapper for safety creating the circular references
/// between two pinned self referential structs.
///
/// # Safety
///
/// This struct *must* not be leaked to the stack using `mem::forget` or any
/// other mechanism that causes the contents of `Self` to be overwritten
/// without `Drop::drop()` running.
/// Doing so creates unsoundness that likely will lead to dereferencing a null
/// pointer.
///
/// Note that it is sound to leak `Self` to the heap using methods including
/// `Box::leak()` because heap allocated data will never be overwritten if it
/// is never freed.
pub struct ValueGuard<T> {
    /// Contains the value being immutably accessed by `RefGuard` and
    /// mutably accessed by `Self`
    ///
    /// This needs to be a cell so that the original immutable alias
    /// to `Self` (given to `RefGuard`) can continue to be referenced after
    /// invalidated by the creation of a mutable alias for `Self::set`.
    data: Cell<T>,
    /// A pointer to a `RefGuard` with read access to `data` to invalidate that
    /// `RefGuard` when `Self` is dropped.
    ref_guard: Cell<Option<NonNull<RefGuard<T>>>>,
    _marker: PhantomPinned,
}

impl<T> ValueGuard<T> {
    /// Creates a new `ValueGuard` containing `data`.
    #[inline]
    pub fn new(data: T) -> Self {
        Self {
            data: Cell::new(data),
            ref_guard: Cell::new(None),
            _marker: PhantomPinned,
        }
    }

    /// Returns a `GuardRegistration`, which can be used to safety link `Self`
    /// to a `RefGuard`.
    #[inline]
    pub fn registration<'a>(self: Pin<&'a Self>) -> GuardRegistration<'a, T> {
        GuardRegistration { value_guard: self }
    }

    /// Sets the internal value stored by `Self`.
    #[inline]
    pub fn set(&self, value: T) {
        self.data.set(value);
    }
}

/// Helper function to invalidate a `ValueGuard`'s `RefGuard` reference
#[inline]
fn invalidate_value_guard<T>(guard: NonNull<ValueGuard<T>>) {
    unsafe { (*guard.as_ptr()).ref_guard.set(None) };
}

impl<T: Copy> ValueGuard<T> {
    /// Gets a copy of the value stored inside this `ValueGuard`.
    #[inline]
    pub fn get(&self) -> T {
        self.data.get()
    }
}

impl<T> Drop for ValueGuard<T> {
    #[inline]
    fn drop(&mut self) {
        if let Some(guard) = self.ref_guard.get() {
            invalidate_ref_guard(guard);
        }
    }
}

/// Weak guard for acquiring read only access to a `ValueGuard`'s value.
///
/// # Safety
///
/// This struct *must* not be leaked to the stack using `mem::forget` or any
/// other mechanism that causes the contents of `Self` to be overwritten
/// without `Drop::drop()` running.
/// Doing so creates unsoundness that likely will lead to dereferencing a null
/// pointer.
///
/// Note that it is sound to leak `Self` to the heap using methods including
/// `Box::leak()` because heap allocated data will never be overwritten if it
/// is never freed.
pub struct RefGuard<T> {
    value_guard: Cell<Option<NonNull<ValueGuard<T>>>>,
    _marker: PhantomPinned,
}

impl<T> RefGuard<T> {
    /// Creates a new `RefGuard` with no reference to a `ValueGuard`.
    #[inline]
    pub fn new() -> Self {
        Self {
            value_guard: Cell::new(None),
            _marker: PhantomPinned,
        }
    }
}

/// Helper function to invalidate a `RefGuard`'s `ValueGuard` reference
#[inline]
fn invalidate_ref_guard<T>(guard: NonNull<RefGuard<T>>) {
    unsafe { (*guard.as_ptr()).value_guard.set(None) };
}

impl<T: Copy> RefGuard<T> {
    /// Gets a copy of the value stored inside the `ValueGuard` this `RefGuard`
    /// references.
    #[inline]
    pub fn get(&self) -> Option<T> {
        self.value_guard
            .get()
            .map(|guard| unsafe { (*guard.as_ptr()).get() })
    }
}

impl<T> Drop for RefGuard<T> {
    #[inline]
    fn drop(&mut self) {
        if let Some(guard) = self.value_guard.get() {
            invalidate_value_guard(guard);
        }
    }
}

impl<T> Default for RefGuard<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Safe api for creating self reference between a pinned `ValueGuard` and
/// `RefGuard` pair.
///
/// This can be acquired with
/// [`ValueGuard::registration()`](ValueGuard::registration).
pub struct GuardRegistration<'a, T> {
    value_guard: Pin<&'a ValueGuard<T>>,
}

impl<'a, T> GuardRegistration<'a, T> {
    /// Binds a provided `slot` to the `self.value_guard`.
    ///
    /// This means they will reference each other, and will invalidate their
    /// references to each other when dropped.
    ///
    /// This method also invalidates the existing references held by the
    /// now-replaced referencees of `slot` and `self.value_guard` to avoid
    /// dangling pointers.
    pub fn register(self, slot: Pin<&'a RefGuard<T>>) {
        // replace slot's value guard with reference to self.value_guard
        // and invalidate slot's old value guard if it exists
        if let Some(old_guard) = slot
            .value_guard
            .replace(Some(self.value_guard.get_ref().into()))
        {
            invalidate_value_guard(old_guard);
        }

        // replace self.value_guard's ref guard with reference to slot
        // and invalidate self.value_guard's old ref guard if it exists
        if let Some(old_guard) = self
            .value_guard
            .ref_guard
            .replace(Some(slot.get_ref().into()))
        {
            invalidate_ref_guard(old_guard);
        }
    }
}

#[cfg(test)]
mod test {
    use std::{mem, pin};

    use super::*;

    #[test]
    fn basic() {
        let weak = pin::pin!(RefGuard::new());
        {
            let strong = pin::pin!(ValueGuard::new(2));
            strong.as_ref().registration().register(weak.as_ref());

            assert_eq!(strong.get(), 2);
            assert_eq!(weak.get(), Some(2));

            strong.as_ref().set(3);
            assert_eq!(strong.get(), 3);
            assert_eq!(weak.get(), Some(3));
        }

        assert_eq!(weak.get(), None);
    }

    #[test]
    fn multiple_registrations() {
        let weak1 = pin::pin!(RefGuard::new());
        let weak2 = pin::pin!(RefGuard::new());
        {
            let strong = pin::pin!(ValueGuard::new(2));
            strong.as_ref().registration().register(weak1.as_ref());

            assert_eq!(strong.get(), 2);
            assert_eq!(weak1.get(), Some(2));

            strong.as_ref().set(3);
            assert_eq!(strong.get(), 3);
            assert_eq!(weak1.get(), Some(3));

            // register next ptr, should invalidate previous weak ref (weak1)
            strong.as_ref().registration().register(weak2.as_ref());
            assert_eq!(weak1.get(), None);
            assert_eq!(weak1.value_guard.get(), None);

            assert_eq!(strong.get(), 3);
            assert_eq!(weak2.get(), Some(3));

            strong.as_ref().set(4);
            assert_eq!(strong.get(), 4);
            assert_eq!(weak2.get(), Some(4));
        }

        assert_eq!(weak1.get(), None);
        assert_eq!(weak2.get(), None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn safe_leak() {
        let strong = Box::pin(ValueGuard::new(10));
        let weak = pin::pin!(RefGuard::new());
        strong.as_ref().registration().register(weak.as_ref());

        // strong is now a ValueGuard on the heap that will never be freed
        // this is sound because it will never be overwritten
        mem::forget(strong);

        assert_eq!(weak.get(), Some(10));
    }
}
