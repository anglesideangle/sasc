#![no_std]

pub use futures_combinators;
pub use futures_core;
use futures_core::ScopedFuture;
pub use futures_derive::async_scoped;
pub use futures_util;

async fn evil() {}

#[async_scoped]
fn inner(a: i32, b: &i32) -> i32 {
    // evil().await;
    1
}

#[async_scoped]
fn test(a: i32, b: &i32) -> i32 {
    futures_derive::block! { 1 + *b; 2 }.await
}

fn test2<'a>(a: i32) {}
