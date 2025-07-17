mod maybe_done;

pub(crate) use maybe_done::*;

use crate::future::ScopedFuture;

// Just a helper function to ensure the futures we're returning all have the
// right implementations.
pub(crate) fn assert_future<'scope, T, F>(future: F) -> F
where
    F: ScopedFuture<'scope, Output = T>,
{
    future
}
