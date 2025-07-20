use futures_core::{ScopedFuture, Wake};
use futures_util::WakeStore;
use futures_util::{MaybeDone, maybe_done};
use std::{cell::Cell, task::Poll};

/// from [futures-concurrency](https://github.com/yoshuawuyts/futures-concurrency/tree/main)
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
    /// This function returns a new future which polls all futures concurrently.
    fn join(self) -> Self::Future;
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
                $(pub $F: Cell<Option<&'scope dyn Wake<'scope>>>,)*
            }
        }

        #[allow(non_snake_case)]
        #[must_use = "futures do nothing unless you `.await` or poll them"]
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
                    self.wakers.$F.set_parent(wake) ;
                    self.refs.$F.replace(Some(&self.wakers.$F));

                    if !self.$F.is_done() {
                        ready &= if self.wakers.$F.take_ready() {
                            // by polling the future, we create our self referentials truct for lifetime 'scope
                            // # SAFETY
                            // unwrap_unchecked is safe because we just put a Some value into our refs.$F
                            // so it is guaranteed to be Some
                            self.$F.poll(unsafe { (&self.refs.$F.get()).unwrap_unchecked() }).is_ready()
                        } else {
                            false
                        };
                    }
                )+

                if ready {
                    Poll::Ready((
                        $(
                            // # SAFETY
                            // `ready == true` when all futures are already
                            // complete or just complete. Once not `MaybeDoneState::Future`, futures transition to `MaybeDoneState::Done`. We don't poll them after, or take their outputs so we know the result of `take_output` must be `Some`
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

#[cfg(test)]
mod tests {
    #![no_std]

    use futures_util::{noop_wake, poll_fn};

    use super::*;

    #[test]
    fn counters() {
        let x1 = Cell::new(0);
        let x2 = Cell::new(0);
        let f1 = poll_fn(|wake| {
            wake.wake();
            x1.set(x1.get() + 1);
            if x1.get() == 4 {
                Poll::Ready(x1.get())
            } else {
                Poll::Pending
            }
        });
        let f2 = poll_fn(|wake| {
            wake.wake();
            x2.set(x2.get() + 1);
            if x2.get() == 5 {
                Poll::Ready(x2.get())
            } else {
                Poll::Pending
            }
        });
        let dummy_waker = noop_wake();
        let join = (f1, f2).join();
        for _ in 0..4 {
            assert_eq!(join.poll(&dummy_waker), Poll::Pending);
        }
        assert_eq!(join.poll(&dummy_waker), Poll::Ready((4, 5)));
    }

    #[test]
    fn never_wake() {
        let f1 = poll_fn(|_| Poll::<i32>::Pending);
        let f2 = poll_fn(|_| Poll::<i32>::Pending);
        let dummy_waker = noop_wake();
        let join = (f1, f2).join();
        for _ in 0..10 {
            assert_eq!(join.poll(&dummy_waker), Poll::Pending);
        }
    }

    #[test]
    fn basic() {
        let f1 = poll_fn(|_| Poll::Ready(1));
        let f2 = poll_fn(|_| Poll::Ready(2));
        let dummy_waker = noop_wake();
        assert_eq!((f1, f2).join().poll(&dummy_waker), Poll::Ready((1, 2)));
    }
}
