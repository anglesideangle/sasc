mod future;

use std::{
    cell::Cell,
    pin::{self, Pin},
    ptr::NonNull,
    task::Poll,
};

use cordyceps::{Linked, list};

/// from yoshuawuyts/futures-concurrency
/// Wait for all futures to complete.
///
/// Awaits multiple futures simultaneously, returning the output of the futures
/// in the same container type they were created once all complete.
pub trait Join<'scope> {
    /// The resulting output type.
    type Output;

    /// The [`Future`] implementation returned by this method.
    type Future: ScopedFuture<'scope, Output = Self::Output>;

    /// Waits for multiple futures to complete.
    ///
    /// Awaits multiple futures simultaneously, returning the output of the futures
    /// in the same container type they we're created once all complete.
    ///
    /// # Examples
    ///
    /// Awaiting multiple futures of the same type can be done using either a vector
    /// or an array.
    /// ```rust
    /// #  futures::executor::block_on(async {
    /// use futures_concurrency::prelude::*;
    ///
    /// // all futures passed here are of the same type
    /// let fut1 = core::future::ready(1);
    /// let fut2 = core::future::ready(2);
    /// let fut3 = core::future::ready(3);
    ///
    /// let outputs = [fut1, fut2, fut3].join().await;
    /// assert_eq!(outputs, [1, 2, 3]);
    /// # })
    /// ```
    ///
    /// In practice however, it's common to want to await multiple futures of
    /// different types. For example if you have two different `async {}` blocks,
    /// you want to `.await`. To do that, you can call `.join` on tuples of futures.
    /// ```rust
    /// #  futures::executor::block_on(async {
    /// use futures_concurrency::prelude::*;
    ///
    /// async fn some_async_fn() -> usize { 3 }
    ///
    /// // the futures passed here are of different types
    /// let fut1 = core::future::ready(1);
    /// let fut2 = async { 2 };
    /// let fut3 = some_async_fn();
    /// //                       ^ NOTE: no `.await` here!
    ///
    /// let outputs = (fut1, fut2, fut3).join().await;
    /// assert_eq!(outputs, (1, 2, 3));
    /// # })
    /// ```
    ///
    /// <br><br>
    /// This function returns a new future which polls all futures concurrently.
    fn join(self) -> Self::Future;
}

// "look at what they need for a fraction of our power" (more efficient join impl is regular join here)
// https://github.com/yoshuawuyts/futures-concurrency/blob/main/src/utils/wakers/array/waker.rs
// possibly copy large portions of futures-concurrency over here

// contains a future that may be finished, safe to poll after ready
enum MaybeReady<'scope, F: ScopedFuture<'scope>> {
    Polling(F),
    Ready(F::Output),
}

impl<'scope, F: ScopedFuture<'scope>> ScopedFuture<'scope> for MaybeReady<'scope, F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &'scope mut dyn Wake) -> Poll<Self::Output> {
        todo!()
    }
}

// TODO bit packing
struct WakeStore {
    ready: bool,
}

impl WakeStore {
    fn read_ready(&mut self) -> bool {
        let out = self.ready;
        self.ready = false;
        out
    }
}

impl Wake for WakeStore {
    fn wake(&mut self) {
        self.ready = true;
    }
}

// field for Join
struct Pollable<'scope, F: ScopedFuture<'scope>> {
    future: MaybeReady<'scope, F>,
    waker: WakeStore,
}

impl<'scope, F: ScopedFuture<'scope>> Pollable<'scope, F> {
    fn new(fut: F) -> Self {
        Self {
            future: MaybeReady::Polling(fut),
            waker: WakeStore { ready: true },
        }
    }
}

// heavily based on https://github.com/yoshuawuyts/futures-concurrency
macro_rules! impl_join_tuple {
    ($mod_name:ident $StructName:ident $($F:ident)+) => {
        pub struct $StructName<'scope, $($F: ScopedFuture<'scope>),+> {
            $($F: Pollable<'scope, $F>,)*
        }

        impl<'scope, $($F: ScopedFuture<'scope>),+> ScopedFuture<'scope> for $StructName<'scope, $($F),+> {
            type Output = ($($F::Output),+);


            fn poll(self: Pin<&mut Self>, cx: &'scope mut dyn Wake) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };

                let ready = true;

                // "loop" through all futures, poll if ready
                $(
                match this.$F.future {
                    MaybeReady::Polling(fut) => {
                        let out = unsafe { Pin::new_unchecked(&mut fut) }.poll(&mut this.$F.waker);
                        if let Poll::Ready(result) = out {
                            // violate pin but that's ok because the future completed
                            this.$F.future = MaybeReady::Ready(result);
                        }
                    },
                    MaybeReady::Ready(_) => {

                    }
                }
                )

                todo!()
            }
        }

        impl<'scope, $($F: ScopedFuture<'scope>),+> Join<'scope> for ($($F),+) {
            type Output = ($($F::Output),*);
            type Future = $StructName<'scope, $($F),+>;

            fn join(self) -> Self::Future {
                let ($($F),+): ($($F),+) = self;
                $StructName { $($F: Pollable::new($F),)* }
            }
        }

        // // Implementation block for the generated struct.
        // impl<$(F),+> $StructName<$(F),+> {
        //     /// Returns the number of generic types the struct was created with.
        //     /// This uses a common macro trick to "count" repetitions by creating
        //     /// an array of stringified identifiers and getting its length at compile time.
        //     const fn generic_type_count() -> usize {
        //         [$(stringify!(F)),*].len()
        //     }

        //     /// Checks if the `count` field is greater than the number of generic types.
        //     pub fn is_count_greater_than_len(&self) -> bool {
        //         self.count as usize > Self::generic_type_count()
        //     }
        // }
    };
}

impl_join_tuple!(join2 Join2 A B);

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

pub trait ScopedStream<'scope> {
    type Item;

    fn poll_next(self: Pin<&mut Self>, cx: &'scope mut dyn Wake) -> Poll<Option<Self::Item>>;
}

// represents an active task, to be used by UnorderedJoinHandle
pub struct Task<'scope, Output, F: ScopedFuture<'scope, Output = Output>> {
    inner: F,
    scope: &'scope UnorderedJoinHandle<'scope, Output>,
    next_active: list::Links<Self>,
}

impl<'scope, Output, F: ScopedFuture<'scope, Output = Output>> Wake for Task<'scope, Output, F> {
    fn wake(&self) {
        // TODO add self to running queue
        // propogate wake up scope
        self.scope.enqueue();
    }
}

// impl<'scope, Output, F: ScopedFuture<'scope, Output = Output>> TaskErasure
//     for Task<'scope, Output, F>
// {
// }

// !Forget
// this is the most annoying data structure ever:
// should it own the tasks?? maybe
//
// a)
//
// Task { &Future, *mut Task }
//
// b)
//
// b is better, use proc macros, no data structures!
//
// <n1, n2, n3 > (o1, o2, o3) etc
// pub struct UnorderedJoinHandle<'scope, Output> {
//     parent_waker: &'scope mut dyn Wake,
//     active_head: Pin<*const dyn TaskErasure>,
//     inactive_head: Pin<*const dyn TaskErasure>,
//     // tasks: [&'scope dyn ScopedFuture<'scope, Output = Output>; N],
// }

// impl<'scope, Output> UnorderedJoinHandle<'scope, Output> {
//     /// adds task to running queue, wakes task
//     pub fn enqueue(&self) {
//         self.parent_waker.wake();
//         todo!()
//     }

//     pub fn spawn(&self) {
//         todo!()
//     }
// }

// should be mandated by !Forget
/// # Soundness
///
/// This is unsound!! Don't use my code.
impl<'scope, const N: usize, Output> Drop for UnorderedJoinHandle<'scope, N, Output> {
    fn drop(&mut self) {
        // TODO sever linked list
    }
}

impl<'scope, const N: usize, Output> ScopedStream<'scope>
    for UnorderedJoinHandle<'scope, N, Output>
{
    type Item = Output;

    fn poll_next(self: Pin<&mut Self>, cx: &'scope mut dyn Wake) -> Poll<Option<Self::Item>> {
        // update parent waker to latest waker
        // unsafe { self.get_mut(|f| &mut f.parent_waker) }.set(cx);
        self.get_mut().parent_waker = cx;

        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
