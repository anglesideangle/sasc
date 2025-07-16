mod combinators;
mod future;
mod utils;

use std::{pin::Pin, task::Poll};

/// from yoshuawuyts/futures-concurrency
/// Wait for all futures to complete.
///
/// Awaits multiple futures simultaneously, returning the output of the futures
/// in the same container type they were created once all complete.

// scoped future combinators:
//
// Join<N>
// TryJoin
// Race
// RaceOk
//
// add Deadline(a, rest) (deadline_against())
// also functionality like (a, b, c).join().race_against(d, e, f)
//
// UnorderedJoinQueueStream? is this VecJoinStream?
// OrderedJoinQueueStream

// pub trait ScopedStream<'scope> {
//     type Item;

//     fn poll_next(self: Pin<&mut Self>, cx: &'scope mut dyn ScopedWake) -> Poll<Option<Self::Item>>;
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
