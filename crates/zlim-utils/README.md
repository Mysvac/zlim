# Platform Independent Extensions

Foundation utilities for the zlim engine.

## num

- `NonMax*` integer wrappers for niche-value optimization, analogous to `NonZero*`.

## str

- Small-buffer-optimized string; re-exports the `smol_str` crate.

## vec

- Small-buffer-optimized vector; re-exports the `fastvec` crate.

## mem

- `Bump` — scoped bump allocator for temporary data (freed on drop).
- `Global` — mutex-protected static bump allocator for `'static` lifetime data.

## mpmc

- Unbounded async multi-producer, multi-consumer channel.
- Lock-free, built on `SegQueue` and `Event`; faster than `async-channel`.
- Multiple `Receiver`s can be cloned and used concurrently.

## mpsc

- Unbounded async multi-producer, single-consumer channel.
- Lock-free, built on `SegQueue` and `Event`; faster than `async-channel`.
- Single `Receiver` (not `Clone`), avoiding contention on the consumer side.

## hash

- Deterministic `HashMap` and `HashSet` backed by `hashbrown`, defaulting to a fixed seed (`FixedState`).
- Custom hashers: `NoopState` (identity hash), `SparseState` (Fibonacci hash for ECS entity IDs).
- Re-exports `foldhash` and `hashbrown`.

## sync

- `SpinLock` — busy-wait mutual exclusion with exponential backoff.
- `SegQueue`, `ArrayQueue`, `ListQueue` — lock-free concurrent queues (ported from crossbeam).
- `Backoff`, `OnceFlag`, `Parallel` — synchronization helpers.

## ext

- `ArrayDeque` — fixed-capacity ring-buffer deque on the stack.
- `BlockList` — block-linked-list queue with idle-block pool.
- `CachePadded` — cache-line-aligned wrapper to reduce false sharing.
- `ThreadLocal` — per-thread storage with bucket-based allocation.
- `TypeMap` — `TypeId`-keyed map with no-op hasher.

## exp

- `SyncUnsafeCell` — backport of unstable `core::cell::SyncUnsafeCell`.
- `SyncView` — backport of unstable `core::sync::Exclusive`.

## macros

- `range_invoke!` — repeated range-based macro invocation.
- `define_atomic_id!` — unique ID generator backed by an atomic counter.
- `once_expr!` — single-execution expression, faster than `Once`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
