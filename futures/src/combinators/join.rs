use crate::{
    future::{ScopedFuture, Wake},
    utils::{MaybeDone, maybe_done},
};
use std::pin::Pin;
use std::task::Poll;

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

/// implements unsafe logic for a set of wakers waking one waker
pub struct WakerArray<'scope, const N: usize> {
    parent_waker: Option<Wake<'scope>>,
    // TODO bit packing
    child_readiness: [bool; N],
    child_wakers: [Wake<'scope>; N],
}

impl<'scope, const N: usize> WakerArray<'scope, N> {
    fn new() -> Self {
        // let mut this = Self {
        //     parent_waker: None,
        //     child_readiness: [false; N],
        //     child_wakers: [|| {}; N],
        // };
        // this.child_wakers = [|x| 2 * x; N];
        // this
    }
}

// would be rly nice if rust had java functional interfaces for wake(&mut Self)

// TODO bit packing
struct WakeStore<'scope> {
    ready: bool,
    parent: Wake<'scope>,
}

impl<'scope> WakeStore<'scope> {
    fn new(parent: Wake<'scope>) -> Self {
        Self {
            parent,
            ready: true,
        }
    }

    fn take_ready(&mut self) -> bool {
        let out = self.ready;
        self.ready = false;
        out
    }
}

// impl ScopedWake for WakeStore<'_> {
//     fn wake(&mut self) {
//         self.ready = true;
//         self.parent.wake();
//     }
// }

// heavily based on https://github.com/yoshuawuyts/futures-concurrency
macro_rules! impl_join_tuple {
    ($StructName:ident $($F:ident)+) => {

        // this exists to work around concatenating idents
        // once https://doc.rust-lang.org/stable/unstable-book/language-features/macro-metavar-expr-concat.html is stable, the $StructName can just contain
        // future_$F and waker_$F
        #[allow(non_snake_case)]
        struct Wakers<'scope> {
            $($F: WakeStore<'scope>,)*
        }

        #[allow(non_snake_case)]
        pub struct $StructName<'scope, $($F: ScopedFuture<'scope>),+> {
            parent_waker: Option<Wake<'scope>>,
            $($F: MaybeDone<'scope, $F>,)*
            wakers: Wakers<'scope>,
        }

        impl<'scope, $($F: ScopedFuture<'scope>),+> ScopedFuture<'scope> for $StructName<'scope, $($F),+> {
            type Output = ($($F::Output),+);

            fn register_wake(self: Pin<&mut Self>, waker: Wake<'scope>) {
                unsafe { self.get_unchecked_mut() }.parent_waker = Some(waker);
            }

            fn poll(self: Pin<&mut Self>) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };

                let mut ready = true;

                // "loop" through all ready futures, poll if ready
                //
                // this combinator is complete when all internal futures have
                // polled to completion
                $(
                    // let $F = unsafe { &mut self.map_unchecked_mut(|f| &mut f.$F) };
                    if let MaybeDone::Future(fut) = &mut this.$F {
                        ready &= if this.wakers.$F.take_ready() {
                            unsafe { Pin::new_unchecked(fut) }.poll().is_ready()
                        } else {
                            false
                        };
                    }
                )+

                if ready {
                    Poll::Ready(($(
                     // unwrap_unchecked is safe here because we know all
                     // futures have been polled to completion
                     // (`MaybeDone::Done`) and have never been converted
                     // to `MaybeDone::Gone`
                     unsafe { Pin::new_unchecked(&mut this.$F).take_output().unwrap_unchecked() },
                    )*))
                } else {
                    Poll::Pending
                }

            }
        }

        // impl<'scope, $($F: ScopedFuture<'scope>),+> Wake<'scope> for $StructName<'scope, $($F),+> {
        //     fn wake(&mut self) {
        //         if let Some(waker) = &mut self.parent_waker {
        //             waker.wake();
        //         };
        //     }
        // }

        impl<'scope, $($F: ScopedFuture<'scope>),+> Join<'scope> for ($($F),+) {
            type Output = ($($F::Output),*);
            type Future = $StructName<'scope, $($F),+>;

            #[allow(non_snake_case)]
            fn join(self) -> Self::Future {
                let ($($F),+): ($($F),+) = self;
                // $StructName {
                //     parent_waker: Option::None,
                //     $($F: maybe_done($F),)*
                //     wakers: $(Wakers { $F: WakeStore::new(&mut Self) }),*
                // }
                todo!()
                // TODO register all wakers
            }
        }
    };
}

impl_join_tuple!(Join2 A B);
