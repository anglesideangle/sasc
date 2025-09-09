use futures_util::LocalWaker;

use crate::wake::WakeArray;
use std::pin::Pin;
use std::task::Poll;

/// from [futures-concurrency](https://github.com/yoshuawuyts/futures-concurrency/tree/main)
/// Wait for the first future to complete.
///
/// Awaits multiple future at once, returning as soon as one completes. The
/// other futures are cancelled.
pub trait Race {
    /// The resulting output type.
    type Output;

    /// The [`ScopedFuture`] implementation returned by this method.
    type Future: futures_core::Future<LocalWaker, Output = Self::Output>;

    /// Wait for the first future to complete.
    ///
    /// Awaits multiple futures at once, returning as soon as one completes. The
    /// other futures are cancelled.
    ///
    /// This function returns a new future which polls all futures concurrently.
    fn race(self) -> Self::Future;
}

pub trait RaceExt<'scope> {
    fn race_with<Fut>(self, other: Fut) -> Race2<Self, Fut>
    where
        Self: Sized + futures_core::Future<LocalWaker>,
        Fut: futures_core::Future<LocalWaker>,
    {
        (self, other).race()
    }
}

impl<'scope, T> RaceExt<'scope> for T where T: futures_core::Future<LocalWaker> {}

macro_rules! impl_race_tuple {
    ($namespace:ident $StructName:ident $OutputsName:ident $($F:ident)+) => {
        mod $namespace {
            #[repr(u8)]
            pub(super) enum Indexes { $($F,)+ }
            pub(super) const LEN: usize = [$(Indexes::$F,)+].len();
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
        pub struct $StructName<$($F: futures_core::Future<LocalWaker>),+> {
            $($F: $F,)*
            wake_array: WakeArray<{$namespace::LEN}>,
        }

        impl<'scope, $($F: futures_core::Future<LocalWaker>),+> futures_core::Future<LocalWaker>
            for $StructName<$($F),+>
        {
            type Output = $OutputsName<$($F::Output,)+>;

            #[allow(non_snake_case)]
            fn poll(self: Pin<&mut Self>, waker: Pin<&LocalWaker>) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };

                let wake_array = unsafe { Pin::new_unchecked(&this.wake_array) };
                $(
                    let mut $F = unsafe { Pin::new_unchecked(&mut this.$F) };
                )+

                wake_array.register_parent(waker);

                $(
                    let index = $namespace::Indexes::$F as usize;
                    let waker = unsafe { wake_array.child_guard_ptr(index).unwrap_unchecked() };

                    // this is safe because we know index < LEN
                    if unsafe { wake_array.take_woken(index).unwrap_unchecked() } {
                        if let Poll::Ready(res) = $F.as_mut().poll(waker) {
                            return Poll::Ready($OutputsName::$F(res));
                        }
                    }
                )+

                Poll::Pending
            }
        }

        impl<'scope, $($F: futures_core::Future<LocalWaker>),+> Race for ($($F),+) {
            type Output = $OutputsName<$($F::Output),*>;
            type Future = $StructName<$($F),+>;

            #[allow(non_snake_case)]
            fn race(self) -> Self::Future {
                let ($($F),+) = self;

                $StructName {
                    $($F: $F,)*
                    wake_array: WakeArray::new(),
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

    use std::pin;

    use futures_core::Future;
    use futures_util::{dummy_guard, poll_fn};

    use crate::wake::local_wake;

    use super::*;

    #[test]
    fn counters() {
        let mut x1 = 0;
        let mut x2 = 0;
        let f1 = poll_fn(|waker| {
            local_wake(waker);
            x1 += 1;
            if x1 == 4 {
                Poll::Ready(x1)
            } else {
                Poll::Pending
            }
        });
        let f2 = poll_fn(|waker| {
            local_wake(waker);
            x2 += 1;
            if x2 == 2 {
                Poll::Ready(x2)
            } else {
                Poll::Pending
            }
        });
        let guard = pin::pin!(dummy_guard());
        let mut race = pin::pin!((f1, f2).race());
        assert_eq!(race.as_mut().poll(guard.as_ref()), Poll::Pending);
        assert_eq!(race.poll(guard.as_ref()), Poll::Ready(RaceOutputs2::B(2)));
    }

    #[test]
    fn never_wake() {
        let f1 = poll_fn(|_| Poll::<i32>::Pending);
        let f2 = poll_fn(|_| Poll::<i32>::Pending);
        let mut race = pin::pin!((f1, f2).race());
        let guard = pin::pin!(dummy_guard());
        for _ in 0..10 {
            assert_eq!(race.as_mut().poll(guard.as_ref()), Poll::Pending);
        }
    }

    #[test]
    fn basic() {
        let f1 = poll_fn(|_| Poll::Ready(1));
        let f2 = poll_fn(|_| Poll::Ready(2));
        let race = pin::pin!(f1.race_with(f2));
        let guard = pin::pin!(dummy_guard());
        assert_eq!(race.poll(guard.as_ref()), Poll::Ready(RaceOutputs2::A(1)));
    }
}
