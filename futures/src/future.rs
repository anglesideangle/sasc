use std::{pin::Pin, task::Poll};

/// A task that can be woken.
///
/// This acts as a handle for a reactor to indicate when a `ScopedFuture` is
/// once again ready to be polled.
pub trait Wake<'scope> {
    fn wake(&self);
}

/// ScopedFuture represents a unit of asynchronous computation that must be
/// polled by an external actor.
///
/// Implementations access a context (`cx: &'scope mut dyn Wake`) to signal
/// they are ready to resume execution.
///
/// A notable difference between `bcsc::ScopedFuture` and `core::task::Future`
/// is the latter cannot safetly ran as a task by an executor without having a
/// 'static lifetime. This is because there is no way for the compiler to
/// guarantee the task doesn't outlive any data, as the executor is free to
/// cancel it (or refuse to) whenever it wants.
///
/// Additionally, because raw/unsafe implementations of `core::task::Waker`
/// effectively do lifetime-erasure, stack-allocated futures cannot prevent
/// unsound behavior from wakers outliving them (even `Forget` would not
/// entirely fix this due to the api).
///
/// In order to avoid unsound behavior, executors must either use Weak<Wake>
/// for safetly losing access to tasks or enforce tasks being stored in
/// `static` pools of memory.
///
/// `ScopedFuture` instead leverages the borrow checker to allow for (less
/// powerful) stack based async execution.
///
/// some more:
/// what occurs in `core::task::Future::poll()` is that the ref to a cx.waker
/// is cloned and stored by a reactor via some method.
///
///
/// The waker is no longer tied to the actual future's lifetime, making it
/// unsound to not have either static tasks or reference counting.
/// To avoid this, we want to use a &'scope waker instead, with 1 waker / task.
///
/// If waker is ownable/cloneable, that erases the lifetime's importance.
/// If the waker is a non clonable mutable reference that lives for 'scope,
/// it cannot be passed into `poll` every time the future is polled, instead it
/// must only be registered once, leading to a register_waker api that is very
/// cumbersome without unsafe poll/unsafe register_waker. Instead, it's easier
/// to use a non clonable immutable reference and have waking occur via
/// interior mutability (this is fine since combinators rely on interior
/// mutability anyway for a 1 parent : many children waker relationship)
pub trait ScopedFuture<'scope> {
    type Output;

    fn poll(self: Pin<&mut Self>, wake: &'scope dyn Wake<'scope>) -> Poll<Self::Output>;
}
