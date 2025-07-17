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
struct WakeStore<'scope> {
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

macro_rules! impl_join_tuple {
    ($StructName:ident $($F:ident)+) => {
        #[allow(non_snake_case)]
        struct Wakers<'scope> {
            $($F: WakeStore<'scope>,)*
        }

        #[allow(non_snake_case)]
        pub struct $StructName<'scope, $($F: ScopedFuture<'scope>),+> {
            $($F: MaybeDone<'scope, $F>,)*
            wakers: Wakers<'scope>,
        }

        impl<'scope, $($F: ScopedFuture<'scope> + 'scope),+> ScopedFuture<'scope>
            for $StructName<'scope, $($F),+>
        {
            type Output = ($($F::Output),+);

            fn poll(self: Pin<&mut Self>, wake: &'scope dyn Wake<'scope>) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                let mut ready = true;

                $(
                    this.wakers.$F.parent = Some(wake);

                    if let MaybeDone::Future(fut) = &mut this.$F {
                        ready &= if this.wakers.$F.take_ready() {
                            unsafe {
                                Pin::new_unchecked(fut).poll(
                                    mem::transmute::<&dyn Wake<'scope>, &'scope dyn Wake<'scope>>(
                                        &this.wakers.$F
                                    )
                                ).is_ready()
                            }
                        } else {
                            false
                        };
                    }
                )+

                if ready {
                    Poll::Ready((
                        $(
                            unsafe {
                                Pin::new_unchecked(&mut this.$F)
                                    .take_output()
                                    .unwrap_unchecked()
                            },
                        )*
                    ))
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
                let ($($F),+) = self;

                $StructName {
                    $($F: maybe_done($F),)*
                    wakers: Wakers { $($F: WakeStore::new(),)* },
                }
            }
        }
    };
}

impl_join_tuple!(Join2 A B);
