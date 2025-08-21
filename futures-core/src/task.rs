// Task: Wake
//
// Wake must not outlive event loop/storage
pub trait Wake<'events> {
    fn wake(&self);
}
