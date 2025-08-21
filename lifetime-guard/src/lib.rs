#![doc = include_str!("../README.md")]
#![no_std]

pub mod guard;

#[cfg(feature = "atomics")]
pub mod atomic_guard;
