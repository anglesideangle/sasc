use crate::wake::WakeArray;
use futures::future::FusedFuture;
use futures::future::MaybeDone;
use futures::future::maybe_done;
use futures_compat::BespokeFutureWrapper;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// from [futures-concurrency](https://github.com/yoshuawuyts/futures-concurrency/tree/main)
/// Wait for all futures to complete.
///
/// Awaits multiple futures simultaneously, returning the output of the futures
/// in the same container type they were created once all complete.
pub trait Join {
    /// The resulting output type.
    type Output;

    /// The [`ScopedFuture`] implementation returned by this method.
    type Future: futures_core::Future<Output = Self::Output>;

    /// Waits for multiple futures to complete.
    ///
    /// Awaits multiple futures simultaneously, returning the output of the futures
    /// in the same container type they we're created once all complete.
    ///
    /// This function returns a new future which polls all futures concurrently.
    fn join(self) -> Self::Future;
}

pub trait JoinExt {
    fn along_with<Fut>(self, other: Fut) -> Join2<Self, Fut>
    where
        Self: Sized + futures_core::Future,
        Fut: futures_core::Future,
    {
        (self, other).join()
    }
}

impl<T> JoinExt for T where T: futures_core::Future {}

macro_rules! impl_join_tuple {
    ($namespace:ident $StructName:ident $($F:ident)+) => {
        mod $namespace {
            #[repr(u8)]
            pub(super) enum Indexes { $($F,)+ }
            pub(super) const LEN: usize = [$(Indexes::$F,)+].len();
        }

        #[allow(non_snake_case)]
        #[must_use = "futures do nothing unless you `.await` or poll them"]
        pub struct $StructName<$($F: futures_core::Future),+> {
            $($F: MaybeDone<BespokeFutureWrapper<$F>>,)*
            wake_array: WakeArray<{$namespace::LEN}>,
        }

        impl<$($F: futures_core::Future),+> futures_core::Future for $StructName<$($F),+>
        {
            type Output = ($($F::Output),+);

            #[allow(non_snake_case)]
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };

                let wake_array = unsafe { Pin::new_unchecked(&this.wake_array) };
                $(
                    // TODO debug_assert_matches is nightly https://github.com/rust-lang/rust/issues/82775
                    debug_assert!(!matches!(this.$F, MaybeDone::Gone), "do not poll futures after they return Poll::Ready");
                    let mut $F = unsafe { Pin::new_unchecked(&mut this.$F) };
                )+

                // extract reference to ValueGuard from Context
                // this is safe because futures_core::Future are isolated
                // from core::future::Future impls and guaranteed to have
                // their cx.wakers represented in the nonstandard format
                unsafe { wake_array.register_parent(futures_compat::waker_to_guard(cx.waker())) }

                let mut ready = true;

                $(
                    let index = $namespace::Indexes::$F as usize;
                    // cx to feed children
                    let waker = unsafe { futures_compat::guard_to_waker(wake_array.child_guard_ptr(index).unwrap_unchecked()) };
                    let mut child_cx = Context::from_waker(&waker);

                    // ready if MaybeDone is Done or just completed (converted to Done)
                    // unsafe / against Future api contract to poll after Gone/Future is finished
                    ready &= if unsafe { wake_array.take_woken(index).unwrap_unchecked() } {
                        $F.as_mut().poll(&mut child_cx).is_ready()
                    } else {
                        $F.is_terminated()
                    };
                )+

                if ready {
                    Poll::Ready((
                        $(
                            // SAFETY:
                            // `ready == true` when all futures are complete.
                            // Once a future is not `MaybeDoneState::Future`, it transitions to `Done`,
                            // so we know the result of `take_output` must be `Some`.
                            unsafe {
                                $F.take_output().unwrap_unchecked()
                            },
                        )*
                    ))
                } else {
                    Poll::Pending
                }
            }
        }

        impl<$($F: futures_core::Future),+> Join for ($($F),+) {
            type Output = ($($F::Output),*);
            type Future = $StructName<$($F),+>;

            #[allow(non_snake_case)]
            fn join(self) -> Self::Future {
                let ($($F),+) = self;

                $StructName {
                    $($F: maybe_done(unsafe { futures_compat::bespoke_future_to_std($F) }),)*
                    wake_array: WakeArray::new(),
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

    use futures_core::{Future, Wake};
    use lifetime_guard::guard::ValueGuard;

    use crate::wake::{DummyWaker, wake_bespoke_waker};

    use super::*;
    use std::cell::Cell;
    use std::future::poll_fn;
    use std::pin;
    use std::ptr::NonNull;

    #[test]
    fn counters() {
        let mut x1 = 0;
        let mut x2 = 0;
        let f1 = poll_fn(|cx| {
            unsafe { wake_bespoke_waker(cx.waker()) };
            x1 += 1;
            if x1 == 4 {
                Poll::Ready(x1)
            } else {
                Poll::Pending
            }
        });
        let f2 = poll_fn(|cx| {
            unsafe { wake_bespoke_waker(cx.waker()) };
            x2 += 1;
            if x2 == 5 {
                Poll::Ready(x2)
            } else {
                Poll::Pending
            }
        });
        let guard = pin::pin!(ValueGuard::new(NonNull::new(
            &mut DummyWaker as *mut dyn Wake,
        )));
        let waker = unsafe { futures_compat::guard_to_waker(guard.as_ref()) };
        let mut cx = Context::from_waker(&waker);
        let mut join = unsafe {
            (
                futures_compat::std_future_to_bespoke(f1),
                futures_compat::std_future_to_bespoke(f2),
            )
        }
        .join();
        let mut pinned = unsafe { Pin::new_unchecked(&mut join) };
        for _ in 0..4 {
            assert_eq!(pinned.as_mut().poll(&mut cx), Poll::Pending);
        }
        assert_eq!(pinned.poll(&mut cx), Poll::Ready((4, 5)));
    }

    // #[test]
    // fn never_wake() {
    //     let f1 = poll_fn(|_| Poll::<i32>::Pending);
    //     let f2 = poll_fn(|_| Poll::<i32>::Pending);
    //     let dummy_waker = noop_wake();
    //     let join = (f1, f2).join();
    //     for _ in 0..10 {
    //         assert_eq!(join.poll(&dummy_waker), Poll::Pending);
    //     }
    // }

    // #[test]
    // fn basic() {
    //     let f1 = poll_fn(|_| Poll::Ready(1));
    //     let f2 = poll_fn(|_| Poll::Ready(2));
    //     let dummy_waker = noop_wake();
    //     assert_eq!(f1.along_with(f2).poll(&dummy_waker), Poll::Ready((1, 2)));
    // }
}
