I will make no_std no_alloc stack allocated async work.

> What does bcsc stand for?

Borrow checked structured concurrency

Normal rust async is borrow checked, but this uses lifetimes (as opposed to spamming ref counting) to enforce structured concurrency in a more elegant, performant, and nostd compatible manner.

> What is wrong with you asa??

I set out to build the best robotics framework in every aspect. I will do that. It's unfortunate that rust doesn't currently make it easy, but I will gladly fix the problems in rust first.

> Ok(())

[multi-task vs intra-task concurrency](https://without.boats/blog/futures-unordered/)

multi-task concurrency extends better to parallelism
- in my case, you can also implement thread affinity for hardware

`FuturesUnordered` requires Arc (std)

[scoped/non-'static futures](https://github.com/rmanoka/async-scoped)

why this (nostd structured concurrency) is unsound/impossible to do safetly

- https://tmandry.gitlab.io/blog/posts/2023-03-01-scoped-tasks/ !!
- https://without.boats/blog/the-scoped-task-trilemma/ !!
- https://conradludgate.com/posts/async-stack !!
- https://cglab.ca/~abeinges/blah/everyone-poops/
- https://sabrinajewson.org/blog/async-drop
- https://blog.yoshuawuyts.com/the-waker-allocation-problem/
- https://faultlore.com/blah/linear-rust/
- https://blog.yoshuawuyts.com/linear-types-one-pager/

problem: i really want this feature, and am fine with unsound code

sound options:
[async nursery](https://github.com/najamelan/async_nursery) - still 'static and not ergonomic api, wraps `FuturesUnordered`
[async-scoped](https://github.com/rmanoka/async-scoped) - wraps `FuturesUnordered`, stores in executor
better?
https://github.com/maroider/async_scoped_task/blob/master/src/lib.rs
unsafe `async_scoped::scope_and_collect` is perfect (unsafe) but uses heap alloc

[moro](https://github.com/nikomatsakis/moro) - wraps `FuturesUnordered`, relies on single threaded for invariants
[task scope](https://docs.rs/task_scope/0.1.1/task_scope/) - scoped tasks but no drop guarantees unless blocking
relevant rfc for Forget
https://github.com/rust-lang/rfcs/pull/3782 !!

outdated tracking issue
https://github.com/rust-lang/compiler-team/issues/727

other similar proposal for Leak
https://zetanumbers.github.io/book/myosotis.html

alternate way of fixing drop issue
https://github.com/Matthias247/rfcs/pull/1

other relevant work/rfc tracking pr
https://github.com/rust-lang/rfcs/pull/2958

why drop?
https://without.boats/blog/wakers-i/
https://without.boats/blog/wakers-ii/

wakers are references to a Task/whatever the executor uses to wrap and enqueue Futures

safe api: [Wake](https://doc.rust-lang.org/beta/std/task/trait.Wake.html)
where `Task: Wake`, wakers are essentially `Weak<Task>` so they can wake the task while it exists (Weak won't get upgraded once the task goes out of scope, so this is safe)
why can't there be a safe api with `Arc`?
`&dyn Wake` doesn't work because concurrency (think: joins) involves multiple wakers for the same task (unless everything is spawned instead of joined!??)
wakers must be cloned, but clone -> Self (Self vtable is unknown through `&dyn Wake` pointer)
ok that explains *const (), but why remove the lifetimes?
not sure?? it seems like it wouldn't make a difference, most futures are static anyway for afformentioned soundness reasons
- what if wakers are an intrusive linked list that the task traverses to cancel when dropped? (requires `!Forget`)/leak safety
- what if wakers were `&dyn Task` with no cloning, and all intra-task concurrency was moved to join handles for scoped spawns
  - also note that stuff like join!() doesn't actually execute the specific future, the outermost task gets woken and then executes all subtasks, which return Pending if they aren't ready
  - intra-task concurrency is evil??
  - still have to wait on concurrent join handles? -> join handles are part of nursery/scope, which stores its own waker-per-task -> subwakers/scope's wakers get called -> scope queues relevant tasks -> call higher level task waker
there is no way to make existing `RawWaker`/`AtomicWaker` api safe because it cannot be "invalidated"?

## What is this project?

New async primitives that disallow intra-task concurrency, clone of `futures` and `futures-concurrency` for the new primitives.

## TODO:
- [x] ScopedFuture
- [ ] static combinators (Join Race etc), see futures-concurrency
- [ ] `#[bsync]` or some compiler ScopedFuture generation
- [ ] growable combinators (eg. `FutureGroup`, `FuturesUnordered`) (require alloc?)
- [ ] unsound (needs `Forget`) multithreading
- [ ] "rethinking async rust"
- [ ] all of the above for streams
- [ ] rfc?

channels: need lifetimed receievers, probably needs `Forget` (arc-like channels would be unsafe)

# Chapter 2:

I am actually addicted to rewriting async rust at this point I need a new hobby

why ch 1 failed:

discussed in [waker allocation problem](https://blog.yoshuawuyts.com/the-waker-allocation-problem/) from earlier: Futures are self referential
- i somehow did not realize this

If you have `Task<'scope>`, which contains `Future<'scope>'` and `Waker<'scope>'`, where `Future::poll(wake: &'scope dyn Wake)`, there needs to be `&'scope` references to owned wakers inside `Task` being passed to owned futures (also in `Task`). This is useful because it prevents reactors (anything that registers a `&'scope dyn Wake`) from outliving the future/task containing them, preventing dangling pointers without `Arc`/alloc.
However, it also is self referential:
- `Task<'scope>` means your task must live `<= 'scope`
- `Task { FutureImpl { Reactor { &'scope dyn Wake} } }` (the task must store a reactor at some point) means the task must live for `>= 'scope`

This is fine, actually! Rust [does](https://sabrinajewson.org/blog/null-lifetime) allow self referential structs*.
The catch is that, as you may have guessed, the task must live for exactly `'scope`. Since it has an immutable reference to parts of itself, and immutable references are disallowed from moving while they are held, the task may not move after the self-reference has been established.

This usually makes self referential structs useless, but... not necessarily in this case

```rust
// suppose rustc generated impls for ScopedFuture
async fn example() {
  let f1 = my_future(1);
  let f2 = my_future(2);
  join!(f1, f2) // all async combinators would need to be macros that create self reference in-place
  // actually this would also work because it isn't being polled
}
```

```rust
// suppose rustc generated impls for ScopedFuture
async fn example() {
  let f1 = my_future(1);
  let f2 = my_future(2);
  join!(f1, f2).await; // no futures passed around, used in place!
  // compiles fine!!
}
```

Cool properties:
- no `Pin`, since the compiler guarantees the future will not move due to self reference for entire lifetime of 'scope
- no unsafe code(?) - no pins

Tradeoffs:
- need tons of interior mutability, since immutable/can't move means `poll` cannot take `&mut self`, cells everywhere
  - nvm lots of unsafe code, but nothing really unsound
- potentially bad error messages? stuff like `join!` will have to output code that manually sets up the waker self ref
