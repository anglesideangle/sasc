use std::{mem, ptr::NonNull, sync::atomic::AtomicPtr};

// Task: Wake
//
// Wake must not outlive event loop/storage
pub trait Wake<'events> {
    fn wake(&self);
    // task can't outlive event loop -> holds &'events
    fn register_waker(&self, waker: &'events Waker);
}

// type Waker<'events> = Option<NonNull<dyn Wake<'events>>>;

pub struct Waker<'events> {
    task: *const dyn Wake<'events>,
}

// wakers must outlive 'task
impl<'events> Waker<'events> {
    pub fn new(task: *const dyn Wake<'events>) -> Self {
        Self { task: task.into() }
    }

    pub fn wake(self) {
        unsafe { self.task.as_ref() }.inspect(|task| task.wake());
    }
}

pub struct WakerRegistration<'events> {
    task: &'events dyn Wake<'events>, // valid for all of 'events
}

impl<'events> WakerRegistration<'events> {
    // slot is valid for all 'events
    pub fn register(self, slot: &'events mut Waker<'events>) {
        // Cast from 'events to 'static
        //
        // # Safety
        //
        // This is safe because the drop guard guarantees that the task ptr (which lives for static)
        // becomes null when the wake is dropped, ensuring the dangling pointer is never dereferenced.
        let dangling_task = unsafe {
            mem::transmute::<&'events dyn Wake<'events>, *const dyn Wake<'events>>(
                self.task,
            )
        };
        slot.task = dangling_task;

        (*self.task).register_waker(slot);
    }
}
