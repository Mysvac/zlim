//! Regression tests for [`TaskPool::scope`] executor driving.
//!
//! `scope` drives the thread-local `LocalExecutor` and the pool's workers.
//! `spawn_to_main` tasks are executed by the fake main thread that
//! multi-threaded mode starts when `designate_main_thread` is not called — so
//! they complete even under `cargo test`, where every test runs on its own
//! thread and there is no real main thread.

use zlim_task::TaskPool;

#[test]
fn scope_pool_tasks_only() {
    let pool = TaskPool::new();
    let mut results = pool.scope(|scope| {
        for i in 0..4 {
            scope.spawn(async move { i });
        }
    });
    results.sort_unstable();
    assert_eq!(results, vec![0, 1, 2, 3]);
}

#[test]
fn scope_main_tasks_only() {
    let pool = TaskPool::new();
    let mut results = pool.scope(|scope| {
        for i in 0..4 {
            scope.spawn_to_main(async move { i });
        }
    });
    results.sort_unstable();
    assert_eq!(results, vec![0, 1, 2, 3]);
}

#[test]
fn scope_mixed_tasks() {
    let pool = TaskPool::new();
    let mut results = pool.scope(|scope| {
        for i in 0..4 {
            if i % 2 == 0 {
                scope.spawn(async move { i });
            } else {
                scope.spawn_to_main(async move { i });
            }
        }
    });
    results.sort_unstable();
    assert_eq!(results, vec![0, 1, 2, 3]);
}

#[test]
fn scope_dependent_main_task() {
    // A pool task completes, then pushes a main-thread task.
    let pool = TaskPool::new();
    let mut results = pool.scope(|scope| {
        scope.spawn(async move {
            scope.spawn_to_main(async move { 42 });
            1
        });
    });
    results.sort_unstable();
    assert_eq!(results, vec![1, 42]);
}

#[test]
fn main_task_spawns_pool_task() {
    // A main-thread task completes, then pushes a pool task.
    let pool = TaskPool::new();
    let mut results = pool.scope(|scope| {
        scope.spawn_to_main(async move {
            scope.spawn(async move { 7 });
            3
        });
    });
    results.sort_unstable();
    assert_eq!(results, vec![3, 7]);
}
