use core::{cell::Cell, marker::PhantomPinned, pin::Pin, ptr::NonNull};

/// Strong guard for granting read access to a single interior mutable value to
/// [`RefGuard`](RefGuard).
///
/// A `ValueGuard`:`RefGuard` relationship is exclusive, and behaves similarly
/// to a single `Rc` and `Weak` pair, but notably does not require heap
/// allocation.
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

    /// Sets the internal value stored by `Self`.
    #[inline]
    pub fn set(&self, value: T) {
        self.data.set(value);
    }

    #[inline]
    fn invalidate_ref_guard(&self) {
        self.ref_guard.set(None);
    }

    #[inline]
    fn replace_ref_guard(&self, ref_guard: Option<NonNull<RefGuard<T>>>) {
        if let Some(guard) = self.ref_guard.replace(ref_guard) {
            unsafe { (*guard.as_ptr()).invalidate_value_guard() };
        }
    }
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
        self.replace_ref_guard(None);
    }
}

/// Weak guard for acquiring read only access to a `ValueGuard`'s value.
///
/// Provides [`WeakGuard::register()`](Self::register) to register a `ValueGuard`
/// to `Self` and vice versa.
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

    #[inline]
    fn invalidate_value_guard(&self) {
        self.value_guard.set(None);
    }

    #[inline]
    fn replace_value_guard(&self, value_guard: Option<NonNull<ValueGuard<T>>>) {
        if let Some(guard) = self.value_guard.replace(value_guard) {
            unsafe { (*guard.as_ptr()).invalidate_ref_guard() }
        }
    }

    /// Binds a pinned `value_guard` to `self`.
    ///
    /// This means they will reference each other, and will invalidate their
    /// references to each other when dropped.
    ///
    /// This method also invalidates the existing references held by the
    /// now-replaced referencees of `self` and `value_guard` to avoid
    /// dangling pointers.
    #[inline]
    pub fn register<'a>(
        self: Pin<&'a RefGuard<T>>,
        value_guard: Pin<&'a ValueGuard<T>>,
    ) {
        value_guard.replace_ref_guard(Some(self.get_ref().into()));
        self.replace_value_guard(Some(value_guard.get_ref().into()));
    }
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
        self.replace_value_guard(None);
    }
}

impl<T: Copy> Default for RefGuard<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use core::{mem, pin};

    extern crate alloc;

    use super::*;

    #[test]
    fn basic() {
        let weak = pin::pin!(RefGuard::new());
        {
            let strong = pin::pin!(ValueGuard::new(2));
            weak.as_ref().register(strong.as_ref());

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
            weak1.as_ref().register(strong.as_ref());

            assert_eq!(strong.get(), 2);
            assert_eq!(weak1.get(), Some(2));

            strong.as_ref().set(3);
            assert_eq!(strong.get(), 3);
            assert_eq!(weak1.get(), Some(3));

            // register next ptr, should invalidate previous weak ref (weak1)
            weak2.as_ref().register(strong.as_ref());
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
        let strong = alloc::boxed::Box::pin(ValueGuard::new(10));
        let weak = pin::pin!(RefGuard::new());
        weak.as_ref().register(strong.as_ref());

        // strong is now a ValueGuard on the heap that will never be freed
        // this is sound because it will never be overwritten
        mem::forget(strong);

        assert_eq!(weak.get(), Some(10));
    }
}
