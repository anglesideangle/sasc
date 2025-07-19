use futures_core::{ScopedFuture, Wake};
use futures_util::{MaybeDone, MaybeDoneState, maybe_done};
use std::cell::UnsafeCell;
use std::sync::atomic::Ordering;
use std::{sync::atomic::AtomicBool, task::Poll};

/// from yoshuawuyts/futures-concurrency
/// Wait for all futures to complete.
///
/// Awaits multiple futures simultaneously, returning the output of the futures
/// in the same container type they were created once all complete.
pub trait Join<'scope> {
    /// The resulting output type.
    type Output;
    /// The [`ScopedFuture`] implementation returned by this method.
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
    parent: UnsafeCell<Option<&'scope dyn Wake<'scope>>>,
    ready: AtomicBool,
}

impl<'scope> WakeStore<'scope> {
    fn new() -> Self {
        Self {
            parent: Option::None.into(),
            ready: true.into(),
        }
    }
    fn take_ready(&self) -> bool {
        self.ready.swap(false, Ordering::SeqCst)
    }
}

impl<'scope> Wake<'scope> for WakeStore<'scope> {
    fn wake(&self) {
        self.ready.swap(true, Ordering::SeqCst);
        if let Some(parent) = unsafe { &*self.parent.get() } {
            parent.wake();
        }
    }
}

macro_rules! impl_join_tuple {
    ($namespace: ident $StructName:ident $($F:ident)+) => {

        mod $namespace {
            use super::*;

            #[allow(non_snake_case)]
            pub struct Wakers<'scope> {
                $(pub $F: WakeStore<'scope>,)*
            }

            // this is so stupid
            #[allow(non_snake_case)]
            pub struct WakerRefs<'scope> {
                $(pub $F: UnsafeCell<Option<&'scope dyn Wake<'scope>>>,)*
            }
        }

        #[allow(non_snake_case)]
        pub struct $StructName<'scope, $($F: ScopedFuture<'scope>),+> {
            $($F: MaybeDone<'scope, $F>,)*
            wakers: $namespace::Wakers<'scope>,
            refs: $namespace::WakerRefs<'scope>,
        }

        impl<'scope, $($F: ScopedFuture<'scope> + 'scope),+> ScopedFuture<'scope>
            for $StructName<'scope, $($F),+>
        {
            type Output = ($($F::Output),+);

            fn poll(&'scope self, wake: &'scope dyn Wake<'scope>) -> Poll<Self::Output> {
                let mut ready = true;

                $(
                    unsafe { self.wakers.$F.parent.get().replace(Some(wake)) };
                    unsafe { self.refs.$F.get().replace(Some(&self.wakers.$F)) };

                    if let MaybeDoneState::Future(fut) = unsafe { self.$F.get_state() } {
                        ready &= if self.wakers.$F.take_ready() {
                            // by polling the future, we create our self referentials truct for lifetime 'scope
                            fut.poll(unsafe { (&*self.refs.$F.get()).unwrap_unchecked() }).is_ready()
                        } else {
                            false
                        };
                    }
                )+

                if ready {
                    Poll::Ready((
                        $(
                            unsafe {
                                self.$F
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
                    wakers: $namespace::Wakers { $($F: WakeStore::new(),)* },
                    refs: $namespace::WakerRefs { $($F: Option::None.into(),)* }
                }
            }
        }
    };
}

impl_join_tuple!(join2 Join2 A B);
impl_join_tuple!(join3 Join3 A B C);
impl_join_tuple!(join4 Join4 A B C D);
impl_join_tuple!(join5 Join5 A B C D E);
impl_join_tuple!(join6 Join6 A B C D E F);
impl_join_tuple!(join7 Join7 A B C D E F G);
impl_join_tuple!(join8 Join8 A B C D E F G H);
impl_join_tuple!(join9 Join9 A B C D E F G H I);
impl_join_tuple!(join10 Join10 A B C D E F G H I J);
impl_join_tuple!(join11 Join11 A B C D E F G H I J K);
impl_join_tuple!(join12 Join12 A B C D E F G H I J K L);
