use std::ptr::NonNull;

use futures_core::{Future, Wake};
use lifetime_guard::{atomic_guard::AtomicValueGuard, guard::ValueGuard};

pub mod maybe_done;

pub type WakePtr = Option<NonNull<dyn Wake>>;
pub type LocalWaker = ValueGuard<WakePtr>;
pub type AtomicWaker = AtomicValueGuard<WakePtr>;

pub(crate) fn assert_future<T, F>(future: F) -> F
where
    F: Future<LocalWaker, Output = T>,
{
    future
}
