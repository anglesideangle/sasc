#![no_std]

pub use futures_combinators;
use futures_compat::{ScopedFutureWrapper, UnscopedFutureWrapper};
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
fn test(a: i32, b: &i32) -> () {
    // evil().await;
    let x = inner(a, &b).await;
    // async {}.await;

    let test_block = futures_derive::block! { 1 + 1; 2 }.await;

    // let test_closure = futures_derive::closure! { |&ab, &cd| ab + cd };

    // let asdf = futures_derive::closure! { |a: &i32| {
    //     *a + b
    // }};
    // let x = asdf(&a).await;
}

fn test2<'a>(a: i32) {}
