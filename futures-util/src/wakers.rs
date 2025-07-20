use std::cell::Cell;

use futures_core::Wake;

pub struct WakeStore<'scope> {
    parent: Cell<Option<&'scope dyn Wake<'scope>>>,
    ready: Cell<bool>,
}

impl<'scope> WakeStore<'scope> {
    pub fn new() -> Self {
        Self {
            parent: Option::None.into(),
            ready: true.into(),
        }
    }

    pub fn set_parent(&self, parent: &'scope dyn Wake<'scope>) {
        self.parent.replace(Some(parent));
    }

    pub fn take_ready(&self) -> bool {
        self.ready.replace(false)
    }
}

impl<'scope> Wake<'scope> for WakeStore<'scope> {
    fn wake(&self) {
        self.ready.replace(true);
        if let Some(parent) = &self.parent.get() {
            parent.wake();
        }
    }
}

pub struct FnWake<F: Fn()>(F);

impl<'scope, F: Fn()> Wake<'scope> for FnWake<F> {
    fn wake(&self) {
        self.0()
    }
}

impl<'scope, F: Fn()> From<F> for FnWake<F> {
    fn from(value: F) -> Self {
        FnWake(value)
    }
}

pub fn noop_wake() -> FnWake<fn()> {
    FnWake(|| {})
}
