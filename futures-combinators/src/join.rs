use futures_core::{ScopedFuture, Wake};
use futures_util::{MaybeDone, MaybeDoneState, maybe_done};
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

struct WakeStore<'scope> {
    parent: Cell<Option<&'scope dyn Wake<'scope>>>,
    ready: Cell<bool>,
}

impl<'scope> WakeStore<'scope> {
    fn new() -> Self {
        Self {
            parent: Option::None.into(),
            ready: true.into(),
        }
    }
    fn take_ready(&self) -> bool {
        self.ready.replace(false)
    }
}

impl<'scope> Wake<'scope> for WakeStore<'scope> {
    fn wake(&self) {
        self.ready.replace(true);
        if let Some(parent) = &self.parent.get() {
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
                    self.wakers.$F.parent.replace(Some(wake)) ;
                    self.refs.$F.replace(Some(&self.wakers.$F));

                    // # SAFETY
                    // `fut` MUST NOT LIVE PAST THIS BLOCK
                    // OTHER MaybeDone METHODS MUTATE `self` AND `fut` HOLDS
                    // IMMUTABLE REFERENCE INVARIANT
                    if let MaybeDoneState::Future(fut) = unsafe { self.$F.get_state() } {
                        ready &= if self.wakers.$F.take_ready() {
                            // by polling the future, we create our self referentials truct for lifetime 'scope
                            // # SAFETY
                            // unwrap_unchecked is safe because we just put a Some value into our refs.$F
                            // so it is guaranteed to be Some
                            fut.poll(unsafe { (&self.refs.$F.get()).unwrap_unchecked() }).is_ready()
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
    use futures_util::poll_fn;

    use super::*;

    #[test]
    fn basic() {
        let f1 = poll_fn(|_| Poll::Ready(1));
        let f2 = poll_fn(|_| Poll::Ready(2));
        let dummy_waker = WakeStore::new();
        assert_eq!((f1, f2).join().poll(&dummy_waker), Poll::Ready((1, 2)));
    }
}
