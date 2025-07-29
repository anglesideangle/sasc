/// Unsound Drop based guard API
use std::{
    cell::Cell,
    mem,
    pin::{Pin, pin},
    ptr::{self, NonNull, dangling},
};

/// must not outlive strong guard
///
/// enforces strong guard doesn't move with ptr
pub struct WeakGuard<'weak, T> {
    strong: Option<&'weak StrongGuard<'weak, T>>,
}

impl<'weak, T> WeakGuard<'weak, T> {
    pub fn register_strong(&mut self, weak: &'weak T) {
        self.strong = Some(weak);
    }
}

impl<'weak, T> Drop for WeakGuard<'weak, StrongGuard<'weak, T>> {
    fn drop(&mut self) {
        // self.weak.
    }
}

/// outlives strong guard
pub struct StrongGuard<'weak, T> {
    strong: *const WeakGuard<'weak, T>,
}

// wakers must outlive 'task
impl<'weak, T> StrongGuard<'weak, T> {
    pub fn new(strong: *const WeakGuard<'weak, T>) -> Self {
        Self {
            strong: task.into(),
        }
    }
}

// pub struct GuardRegistration<'weak, T> {
//     task: &'weak StrongGuard<'weak, T>, // valid for all of 'weak
// }

// impl<'weak> GuardRegistration<'weak> {
//     // slot is valid for all 'weak
//     pub fn register(self, slot: &'weak mut StrongGuard<'weak>) {
//         // Cast from 'weak to 'static
//         //
//         // # Safety
//         //
//         // This is safe because the drop guard guarantees that the task ptr (which lives for static)
//         // becomes null when the wake is dropped, ensuring the dangling pointer is never dereferenced.
//         let dangling_task = unsafe {
//             mem::transmute::<
//                 &'weak dyn StrongGuard<'weak>,
//                 *const dyn StrongGuard<'weak>,
//             >(self.task)
//         };
//         slot.strong = dangling_task;

//         (*self.task).register_waker(slot);
//     }
// }

pub struct ValueGuard<T> {
    data: T,
    ref_guard: Cell<*const RefGuard<T>>,
}

impl<T> ValueGuard<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            ref_guard: Cell::new(ptr::null()),
        }
    }

    pub fn set(&mut self, value: T) {
        self.data = value;
    }
}

impl<T: Copy> ValueGuard<T> {
    pub fn get(&self) -> T {
        self.data
    }
}

impl<T> Drop for ValueGuard<T> {
    fn drop(&mut self) {
        unsafe {
            self.ref_guard
                .get()
                .as_ref()
                .inspect(|guard| guard.value_guard.set(ptr::null()))
        };
    }
}

pub struct RefGuard<T> {
    value_guard: Cell<*const ValueGuard<T>>,
}

impl<T> RefGuard<T> {
    pub fn new() -> Self {
        Self {
            value_guard: Cell::new(ptr::null()),
        }
    }
}

impl<T: Copy> RefGuard<T> {
    pub fn get(&self) -> Option<T> {
        unsafe { self.value_guard.get().as_ref().map(|ptr| ptr.data) }
    }
}

impl<T> Drop for RefGuard<T> {
    fn drop(&mut self) {
        unsafe { self.value_guard.get().as_ref() }
            .inspect(|guard| guard.ref_guard.set(ptr::null()));
    }
}

pub struct GuardRegistration<'a, T> {
    value_guard: Pin<&'a ValueGuard<T>>,
}

impl<'a, T> GuardRegistration<'a, T> {
    pub fn from_guard(value_guard: Pin<&'a ValueGuard<T>>) -> Self {
        Self { value_guard }
    }

    pub fn register(self, slot: Pin<&'a mut RefGuard<T>>) {
        // register new ptrs
        let old_value_guard = slot
            .value_guard
            .replace(self.value_guard.get_ref() as *const ValueGuard<T>);

        let old_ref_guard = self
            .value_guard
            .ref_guard
            .replace(slot.into_ref().get_ref());

        // annul old ptrs
        unsafe { old_value_guard.as_ref() }
            .inspect(|guard| guard.ref_guard.set(ptr::null()));
        unsafe { old_ref_guard.as_ref() }
            .inspect(|guard| guard.value_guard.set(ptr::null()));
    }
}
