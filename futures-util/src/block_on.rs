use std::{
    pin::{self, Pin},
    task::Poll,
};

use crate::{LocalWaker, dummy_guard};

pub fn block_on<F: futures_core::Future<LocalWaker>>(
    mut f: Pin<&mut F>,
) -> F::Output {
    let dummy_guard = pin::pin!(dummy_guard());
    loop {
        if let Poll::Ready(out) = f.as_mut().poll(dummy_guard.as_ref()) {
            return out;
        }
    }
}
