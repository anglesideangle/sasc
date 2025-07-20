mod maybe_done;
mod poll_fn;
mod wakers;

use futures_core::ScopedFuture;
pub use maybe_done::*;
pub use poll_fn::poll_fn;
pub use wakers::*;

// Just a helper function to ensure the futures we're returning all have the
// right implementations.
pub(crate) fn assert_future<'scope, T, F>(future: F) -> F
where
    F: ScopedFuture<'scope, Output = T>,
{
    future
}
