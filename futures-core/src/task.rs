/// A task that can be woken.
///
/// This acts as a handle for a reactor to indicate when a `ScopedFuture` is
/// once again ready to be polled.
pub trait Wake<'scope> {
    fn wake(&self);
}
