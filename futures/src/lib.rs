#![no_std]

pub use futures_combinators;
pub use futures_core;
pub use futures_derive::async_scoped;
pub use futures_util;

async fn evil() {}

#[async_scoped]
fn test<'a>(a: i32, b: &i32) {
    // evil().await
}

fn test2<'a>(a: i32) {}
