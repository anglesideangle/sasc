pub trait Wake {
    fn wake(&mut self);
}

/// ScopedFuture represents a unit of asynchronous computation that must be
/// polled by an external actor.
///
///
pub trait ScopedFuture<'scope> {
    type Output;

    // TODO make new Context with &'a mut dyn Wake field
    fn poll(self: Pin<&mut Self>, cx: &'scope mut dyn Wake) -> Poll<Self::Output>;
}
