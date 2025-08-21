use core::{cell::Cell, marker::PhantomPinned, pin::Pin, ptr::NonNull};

use critical_section::Mutex;

struct RawValueGuard<T> {
    /// Contains the value being immutably accessed by `RefGuard` and
    /// mutably accessed by `Self`
    ///
    /// This needs to be a cell so that the original immutable alias
    /// to `Self` (given to `RefGuard`) can continue to be referenced after
    /// invalidated by the creation of a mutable alias for `Self::set`.
    data: Cell<T>,
    /// A pointer to a `RefGuard` with read access to `data` to invalidate that
    /// `RefGuard` when `Self` is dropped.
    ref_guard: Cell<Option<NonNull<AtomicRefGuard<T>>>>,
}

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
pub struct AtomicValueGuard<T> {
    /// Mutex is unfortunately necessary because the replace operation requires
    /// confirming if the ptr is valid, meaning it's a two instruction process
    /// and can't be done with atomic compare-and-swap instructions
    mutex: Mutex<RawValueGuard<T>>,
    _marker: PhantomPinned,
}

impl<T> AtomicValueGuard<T> {
    /// Creates a new `ValueGuard` containing `data`.
    #[inline]
    pub fn new(data: T) -> Self {
        Self {
            mutex: Mutex::new(RawValueGuard {
                data: Cell::new(data),
                ref_guard: Cell::new(None),
            }),
            _marker: PhantomPinned,
        }
    }

    /// Sets the internal value stored by `Self`.
    #[inline]
    pub fn set(&self, value: T) {
        critical_section::with(|cs| self.mutex.borrow(cs).data.set(value));
    }

    #[inline]
    fn invalidate_ref_guard(&self) {
        critical_section::with(|cs| self.mutex.borrow(cs).ref_guard.set(None));
    }

    #[inline]
    fn replace_ref_guard(&self, ref_guard: Option<NonNull<AtomicRefGuard<T>>>) {
        critical_section::with(|cs| {
            if let Some(guard) =
                self.mutex.borrow(cs).ref_guard.replace(ref_guard)
            {
                unsafe { (*guard.as_ptr()).invalidate_value_guard() }
            }
        });
    }
}

impl<T: Copy> AtomicValueGuard<T> {
    /// Gets a copy of the value stored inside this `ValueGuard`.
    #[inline]
    pub fn get(&self) -> T {
        critical_section::with(|cs| self.mutex.borrow(cs).data.get())
    }
}

impl<T> Drop for AtomicValueGuard<T> {
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
pub struct AtomicRefGuard<T> {
    value_guard: Cell<Option<NonNull<AtomicValueGuard<T>>>>,
    _marker: PhantomPinned,
}

impl<T> AtomicRefGuard<T> {
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
    fn replace_value_guard(
        &self,
        value_guard: Option<NonNull<AtomicValueGuard<T>>>,
    ) {
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
        self: Pin<&'a AtomicRefGuard<T>>,
        value_guard: Pin<&'a AtomicValueGuard<T>>,
    ) {
        value_guard.replace_ref_guard(Some(self.get_ref().into()));
        self.replace_value_guard(Some(value_guard.get_ref().into()));
    }
}

impl<T: Copy> AtomicRefGuard<T> {
    /// Gets a copy of the value stored inside the `ValueGuard` this `RefGuard`
    /// references.
    #[inline]
    pub fn get(&self) -> Option<T> {
        self.value_guard
            .get()
            .map(|guard| unsafe { (*guard.as_ptr()).get() })
    }
}

impl<T> Drop for AtomicRefGuard<T> {
    #[inline]
    fn drop(&mut self) {
        self.replace_value_guard(None);
    }
}

impl<T> Default for AtomicRefGuard<T> {
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
        let weak = pin::pin!(AtomicRefGuard::new());
        {
            let strong = pin::pin!(AtomicValueGuard::new(2));
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
        let weak1 = pin::pin!(AtomicRefGuard::new());
        let weak2 = pin::pin!(AtomicRefGuard::new());
        {
            let strong = pin::pin!(AtomicValueGuard::new(2));
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
        let strong = alloc::boxed::Box::pin(AtomicValueGuard::new(10));
        let weak = pin::pin!(AtomicRefGuard::new());
        weak.as_ref().register(strong.as_ref());

        // strong is now a ValueGuard on the heap that will never be freed
        // this is sound because it will never be overwritten
        mem::forget(strong);

        assert_eq!(weak.get(), Some(10));
    }
}
