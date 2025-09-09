#![no_std]

use futures_compat::LocalWaker;
use futures_derive::async_function;

async fn evil() {}

#[async_function]
fn inner(a: i32, b: &i32) -> i32 {
    // evil().await;
    1
}

#[async_function]
fn test(a: i32, b: &i32) -> i32 {
    futures_derive::async_block! { let _ = 1 + *b; 2 }.await
}

// fn test2<'a>(a: i32) {}
