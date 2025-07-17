mod maybe_done;

use futures_core::ScopedFuture;
pub use maybe_done::*;

// Just a helper function to ensure the futures we're returning all have the
// right implementations.
pub(crate) fn assert_future<'scope, T, F>(future: F) -> F
where
    F: ScopedFuture<'scope, Output = T>,
{
    future
}
