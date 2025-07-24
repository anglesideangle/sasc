use crate::wake::ScopedWaker;

/// A task that can be woken.
///
/// This acts as a handle for a reactor to indicate when a `ScopedFuture` is
/// once again ready to be polled.
pub trait Wake<'scope> {
    fn wake(&self);
}

impl<'scope> From<&'scope dyn Wake<'scope>> for ScopedWaker<'scope> {
    fn from(value: &'scope dyn Wake) -> Self {}
}

fn raw_waker<'scope, W: Wake<'scope> + Send + Sync>(
    waker: &'scope dyn Wake,
) -> RawWaker {
    // Increment the reference count of the arc to clone it.

    //

    // The #[inline(always)] is to ensure that raw_waker and clone_waker are

    // always generated in the same code generation unit as one another, and

    // therefore that the structurally identical const-promoted RawWakerVTable

    // within both functions is deduplicated at LLVM IR code generation time.

    // This allows optimizing Waker::will_wake to a single pointer comparison of

    // the vtable pointers, rather than comparing all four function pointers

    // within the vtables.

    #[inline(always)]

    unsafe fn clone_waker<W: Wake + Send + Sync + 'static>(
        waker: *const (),
    ) -> RawWaker {
        unsafe { Arc::increment_strong_count(waker as *const W) };

        RawWaker::new(
            waker,
            &RawWakerVTable::new(
                clone_waker::<W>,
                wake::<W>,
                wake_by_ref::<W>,
                drop_waker::<W>,
            ),
        )
    }

    // Wake by value, moving the Arc into the Wake::wake function

    unsafe fn wake<W: Wake + Send + Sync + 'static>(waker: *const ()) {
        let waker = unsafe { Arc::from_raw(waker as *const W) };

        <W as Wake>::wake(waker);
    }

    // Wake by reference, wrap the waker in ManuallyDrop to avoid dropping it

    unsafe fn wake_by_ref<W: Wake + Send + Sync + 'static>(waker: *const ()) {
        let waker =
            unsafe { ManuallyDrop::new(Arc::from_raw(waker as *const W)) };

        <W as Wake>::wake_by_ref(&waker);
    }

    // Decrement the reference count of the Arc on drop

    unsafe fn drop_waker<W: Wake + Send + Sync + 'static>(waker: *const ()) {
        unsafe { Arc::decrement_strong_count(waker as *const W) };
    }

    RawWaker::new(
        Arc::into_raw(waker) as *const (),
        &RawWakerVTable::new(
            clone_waker::<W>,
            wake::<W>,
            wake_by_ref::<W>,
            drop_waker::<W>,
        ),
    )
}
