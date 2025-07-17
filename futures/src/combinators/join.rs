use crate::{
    future::{ScopedFuture, Wake},
    utils::{MaybeDone, maybe_done},
};
use std::mem;
use std::{pin::Pin, sync::atomic::Ordering};
use std::{sync::atomic::AtomicBool, task::Poll};

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

// struct Waker<'scope> {
//     parent_waker: Wake<'scope>,
// }

// impl<'scope> Waker<'scope> {
//     fn wake(&mut self) {
//         (*self.parent_waker)();
//     }
// }

// /// implements unsafe logic for a set of wakers waking one waker
// pub struct WakerArray<'scope, const N: usize> {
//     parent_waker: Option<&'scope dyn Wake<'scope>>,
//     // TODO bit packing
//     child_readiness: [bool; N],
//     pub child_wakers: Option<[Waker<'scope>; N]>,
// }

// impl<'scope, const N: usize> WakerArray<'scope, N> {
//     fn new() -> Self {
//         Self {
//             parent_waker: None,
//             child_readiness: [false; N],
//             child_wakers: None,
//         }
//     }

// fn register_parent_wake(&mut self, wake: Wake<'scope>) {
//     self.parent_waker = Some(wake);
//     self.child_wakers = Some(
//         [Waker {
//             parent_waker: &self.parent_waker,
//         }; N],
//     );
// }
// }

// would be rly nice if rust had java functional interfaces for wake(&mut Self)

struct WakeStore<'scope> {
    // no extra storage bc None is 0x000 ptr
    parent: Option<&'scope dyn Wake<'scope>>,
    ready: AtomicBool,
}

impl<'scope> WakeStore<'scope> {
    fn new() -> Self {
        Self {
            parent: Option::None,
            ready: true.into(),
        }
    }

    fn take_ready(&mut self) -> bool {
        self.ready.swap(false, Ordering::SeqCst)
    }
}

impl<'scope> Wake<'scope> for WakeStore<'scope> {
    fn wake(&self) {
        self.ready.swap(true, Ordering::SeqCst);
        if let Some(parent) = self.parent {
            parent.wake();
        }
    }
}

// heavily based on https://github.com/yoshuawuyts/futures-concurrency
macro_rules! impl_join_tuple {
    ($StructName:ident $($F:ident)+) => {

        // this exists to work around concatenating idents
        // once https://doc.rust-lang.org/stable/unstable-book/language-features/macro-metavar-expr-concat.html is stable, the $StructName can just contain
        // future_$F and waker_$F
        #[allow(non_snake_case)]
        struct Wakers<'scope> {
            // inefficient, needs tt muncher for actual [T; LEN] traversal, fewer cache misses
            $($F: WakeStore<'scope>,)*
        }

        #[allow(non_snake_case)]
        pub struct $StructName<'scope, $($F: ScopedFuture<'scope>),+> {
            // parent_waker: Option<&'scope dyn Wake>,
            $($F: MaybeDone<'scope, $F>,)*
            wakers: Wakers<'scope>,
        }

        impl<'scope, $($F: ScopedFuture<'scope> + 'scope),+> ScopedFuture<'scope> for $StructName<'scope, $($F),+>
        {
            type Output = ($($F::Output),+);

            fn poll(self: Pin<&mut Self>, wake: &'scope dyn Wake<'scope>) -> Poll<Self::Output>
            {
                let this = unsafe { self.get_unchecked_mut() };

                let mut ready = true;

                $(
                    this.wakers.$F.parent = Some(wake);

                    if let MaybeDone::Future(fut) = &mut this.$F {
                        ready &= if this.wakers.$F.take_ready() {
                            unsafe {
                                Pin::new_unchecked(fut).poll(mem::transmute(&this.wakers.$F as &dyn Wake)).is_ready()
                            }
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

        impl<'scope, $($F: ScopedFuture<'scope> + 'scope),+> Join<'scope> for ($($F),+) {
            type Output = ($($F::Output),*);
            type Future = $StructName<'scope, $($F),+>;

            #[allow(non_snake_case)]
            fn join(self) -> Self::Future {
                let ($($F),+): ($($F),+) = self;
                $StructName {
                    // parent_waker: Option::None,
                    $($F: maybe_done($F),)*
                    wakers: Wakers { $($F: WakeStore::new(),)* }
                }
            }
        }
    };
}

impl_join_tuple!(Join2 A B);
