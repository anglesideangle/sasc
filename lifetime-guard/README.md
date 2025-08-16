# Lifetime Guard

`lifetime-guard` provides `ValueGuard` and `RefGuard` structs to allow for
weak references to interior mutable values, similar to a singular pair of
`Rc` and `Weak`, but without heap allocation.

## Example Usage

```rust
use std::pin;
use lifetime_guard::{ ValueGuard, RefGuard };

let weak = pin::pin!(RefGuard::new());
{
    let strong = pin::pin!(ValueGuard::new(0));
    strong.as_ref().registration().register(weak.as_ref());

    assert_eq!(strong.get(), 0);
    assert_eq!(weak.get(), Some(0));

    strong.as_ref().set(1);
    assert_eq!(strong.get(), 1);
    assert_eq!(weak.get(), Some(1));
}
assert_eq!(weak.get(), None);
```

# Safety

You *may not* leak any instance of either `ValueGuard` or `RefGuard` to the
stack using `mem::forget()` or any other mechanism that causes thier
contents to be overwritten without `Drop::drop()` running.
Doing so creates unsoundness that likely will lead to dereferencing a null
pointer.

Doing so creates unsoundness that likely will lead to dereferencing a null
pointer. See the
[Forget marker trait](https://github.com/rust-lang/rfcs/pull/3782) rfc for
progress on making interfaces that rely on not being leaked sound.

Note that it is sound to leak `ValueGuard` and `RefGuard` to the heap using
methods including `Box::leak()` because heap allocated data will never be
overwritten if it is never freed.

