use futures_core::{ScopedFuture, Wake};
use futures_util::WakeStore;
use std::{cell::Cell, task::Poll};

/// from [futures-concurrency](https://github.com/yoshuawuyts/futures-concurrency/tree/main)
/// Wait for the first future to complete.
///
/// Awaits multiple future at once, returning as soon as one completes. The
/// other futures are cancelled.
pub trait Race<'scope> {
    /// The resulting output type.
    type Output;

    /// The [`ScopedFuture`] implementation returned by this method.
    type Future: ScopedFuture<'scope, Output = Self::Output>;

    /// Wait for the first future to complete.
    ///
    /// Awaits multiple futures at once, returning as soon as one completes. The
    /// other futures are cancelled.
    ///
    /// This function returns a new future which polls all futures concurrently.
    fn race(self) -> Self::Future;
}

pub trait RaceExt<'scope> {
    fn race_with<Fut>(self, other: Fut) -> Race2<'scope, Self, Fut>
    where
        Self: Sized + 'scope + ScopedFuture<'scope>,
        Fut: ScopedFuture<'scope> + 'scope,
    {
        (self, other).race()
    }
}

impl<'scope, T> RaceExt<'scope> for T where T: ScopedFuture<'scope> {}

macro_rules! impl_race_tuple {
    ($namespace:ident $StructName:ident $OutputsName:ident $($F:ident)+) => {
        mod $namespace {
            use super::*;

            #[allow(non_snake_case)]
            pub struct Wakers<'scope> {
                $(pub $F: WakeStore<'scope>,)*
            }

            #[allow(non_snake_case)]
            pub struct WakerRefs<'scope> {
                $(pub $F: Cell<Option<&'scope dyn Wake<'scope>>>,)*
            }
        }

        pub enum $OutputsName<$($F,)+> {
            $($F($F),)+
        }

        impl<$($F: std::fmt::Debug,)+> std::fmt::Debug for $OutputsName<$($F,)+> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {$(
                    Self::$F(x) =>
                    f.debug_tuple(std::stringify!($F))
                        .field(x)
                        .finish(),
                )+}
            }
        }

        impl<$($F: PartialEq,)+> PartialEq for $OutputsName<$($F,)+> {
            fn eq(&self, other: &Self) -> bool {
                match (self, other) {
                    $((Self::$F(a), Self::$F(b)) => a == b,)+
                    _ => false
                }
            }
        }

        #[allow(non_snake_case)]
        #[must_use = "futures do nothing unless you `.await` or poll them"]
        pub struct $StructName<'scope, $($F: ScopedFuture<'scope>),+> {
            $($F: $F,)*
            wakers: $namespace::Wakers<'scope>,
            refs: $namespace::WakerRefs<'scope>,
        }

        impl<'scope, $($F: ScopedFuture<'scope> + 'scope),+> ScopedFuture<'scope>
            for $StructName<'scope, $($F),+>
        {
            type Output = $OutputsName<$($F::Output,)+>;

            fn poll(&'scope self, wake: &'scope dyn Wake<'scope>) -> Poll<Self::Output> {
                $(
                    self.wakers.$F.set_parent(wake);
                    self.refs.$F.replace(Some(&self.wakers.$F));

                    if self.wakers.$F.take_ready() {
                        // By polling the future, we create our self-referential structure for lifetime `'scope`.
                        //
                        // SAFETY:
                        // `unwrap_unchecked` is safe because we just inserted `Some` into `refs.$F`,
                        // so it is guaranteed to be `Some`.
                        if let Poll::Ready(res) = self.$F.poll(unsafe { (&self.refs.$F.get()).unwrap_unchecked() }) {
                            return Poll::Ready($OutputsName::$F(res));
                        }
                    }
                )+

                Poll::Pending
            }
        }

        impl<'scope, $($F: ScopedFuture<'scope> + 'scope),+> Race<'scope> for ($($F),+) {
            type Output = $OutputsName<$($F::Output),*>;
            type Future = $StructName<'scope, $($F),+>;

            #[allow(non_snake_case)]
            fn race(self) -> Self::Future {
                let ($($F),+) = self;

                $StructName {
                    $($F: $F,)*
                    wakers: $namespace::Wakers {
                        $($F: WakeStore::new(),)*
                    },
                    refs: $namespace::WakerRefs {
                        $($F: Option::None.into(),)*
                    },
                }
            }
        }
    };
}

impl_race_tuple!(race2 Race2 RaceOutputs2 A B);
impl_race_tuple!(race3 Race3 RaceOutputs3 A B C);
impl_race_tuple!(race4 Race4 RaceOutputs4 A B C D);
impl_race_tuple!(race5 Race5 RaceOutputs5 A B C D E);
impl_race_tuple!(race6 Race6 RaceOutputs6 A B C D E F);
impl_race_tuple!(race7 Race7 RaceOutputs7 A B C D E F G);
impl_race_tuple!(race8 Race8 RaceOutputs8 A B C D E F G H);
impl_race_tuple!(race9 Race9 RaceOutputs9 A B C D E F G H I);
impl_race_tuple!(race10 Race10 RaceOutputs10 A B C D E F G H I J);
impl_race_tuple!(race11 Race11 RaceOutputs11 A B C D E F G H I J K);
impl_race_tuple!(race12 Race12 RaceOutputs12 A B C D E F G H I J K L);

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
            if x2.get() == 2 {
                Poll::Ready(x2.get())
            } else {
                Poll::Pending
            }
        });
        let dummy_waker = noop_wake();
        let join = (f1, f2).race();
        assert_eq!(join.poll(&dummy_waker), Poll::Pending);
        assert_eq!(join.poll(&dummy_waker), Poll::Ready(RaceOutputs2::B(2)));
    }

    #[test]
    fn never_wake() {
        let f1 = poll_fn(|_| Poll::<i32>::Pending);
        let f2 = poll_fn(|_| Poll::<i32>::Pending);
        let dummy_waker = noop_wake();
        let join = (f1, f2).race();
        for _ in 0..10 {
            assert_eq!(join.poll(&dummy_waker), Poll::Pending);
        }
    }

    #[test]
    fn basic() {
        let f1 = poll_fn(|_| Poll::Ready(1));
        let f2 = poll_fn(|_| Poll::Ready(2));
        let dummy_waker = noop_wake();
        assert_eq!(
            f1.race_with(f2).poll(&dummy_waker),
            Poll::Ready(RaceOutputs2::A(1))
        );
    }
}
