use std::task::Poll;

mod task;

pub use crate::task::Wake;

/// ScopedFuture represents a unit of asynchronous computation that must be
/// polled by an external actor.
///
/// Implementations access a context (`cx: &'scope dyn Wake`) to signal
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
/// The waker is no longer tied to the actual future's lifetime, making it
/// unsound to not have either static tasks or reference counting.
/// To avoid this, we want to use a &'scope waker instead, with 1 waker / task.
pub trait ScopedFuture<'scope> {
    type Output;

    /// as soon as poll is called, the struct becomes self-referential,
    /// effectively pinned until dropped (or forgotten....D; )
    fn poll(
        self: &'scope Self,
        wake: &'scope dyn Wake<'scope>,
    ) -> Poll<Self::Output>;
}
