//! Single-threaded benchmark: zlim-task vs bevy_tasks.
//!
//! `zlim-task` is compiled with the `single_thread` feature.
//! Run with `cargo run --release`.

use std::time::Instant;

const TASKS: usize = 100_000;
const WARMUP: usize = 3;
const RUNS: usize = 5;

fn main() {
    println!("single-threaded — {TASKS} tasks, {RUNS} runs (after {WARMUP} warmups)\n");

    bench_scope_tiny();
    bench_scope_heavy();
    bench_spawn_drive();
}

// -----------------------------------------------------------------------------
// helpers
// -----------------------------------------------------------------------------

fn time<R>(label: &str, mut f: impl FnMut() -> R) -> std::time::Duration {
    for _ in 0..WARMUP {
        core::hint::black_box(f());
    }
    let start = Instant::now();
    for _ in 0..RUNS {
        core::hint::black_box(f());
    }
    let each = start.elapsed() / RUNS as u32;
    println!("  {label:<24} {each:>8.2?}");
    each
}

// -----------------------------------------------------------------------------
// tiny task — scheduling overhead
// -----------------------------------------------------------------------------

fn bench_scope_tiny() {
    println!("--- scope spawn (tiny task) ---");

    let zp = zlim_task::TaskPool::new();
    let zt = time("zlim-task", || {
        zp.scope(|s| {
            for i in 0..TASKS {
                s.spawn(async move { i });
            }
        })
    });

    let bp = bevy_tasks::TaskPool::new();
    let bt = time("bevy_tasks", || {
        bp.scope(|s| {
            for i in 0..TASKS {
                s.spawn(async move { i });
            }
        })
    });

    println!(
        "  → zlim / bevy = {:.2}x\n",
        bt.as_secs_f64() / zt.as_secs_f64()
    );
}

// -----------------------------------------------------------------------------
// heavier task
// -----------------------------------------------------------------------------

fn bench_scope_heavy() {
    println!("--- scope spawn (compute) ---");

    let zp = zlim_task::TaskPool::new();
    let zt = time("zlim-task", || {
        zp.scope(|s| {
            for i in 0..TASKS {
                s.spawn(async move { (0..2000).fold(i, |a, b| core::hint::black_box(a ^ b)) });
            }
        })
    });

    let bp = bevy_tasks::TaskPool::new();
    let bt = time("bevy_tasks", || {
        bp.scope(|s| {
            for i in 0..TASKS {
                s.spawn(async move { (0..2000).fold(i, |a, b| core::hint::black_box(a ^ b)) });
            }
        })
    });

    println!(
        "  → zlim / bevy = {:.2}x\n",
        bt.as_secs_f64() / zt.as_secs_f64()
    );
}

// -----------------------------------------------------------------------------
// fire-and-forget + manual driving (the zlim single-threaded model)
// -----------------------------------------------------------------------------

fn bench_spawn_drive() {
    println!("--- spawn + run_local (fire-and-forget) ---");

    let zp = zlim_task::TaskPool::new();
    let zt = time("zlim-task", || {
        for i in 0..TASKS {
            zp.spawn(async move { i }).detach();
        }
        zlim_task::run_local();
    });

    let bp = bevy_tasks::TaskPool::new();
    let bt = time("bevy_tasks", || {
        for i in 0..TASKS {
            bp.spawn(async move { i }).detach();
        }
        // bevy_tasks does not expose a run_local equivalent in its public API;
        // we rely on the internal tick in single-threaded mode (if available).
        // This is not a perfect comparison.
    });

    println!(
        "  → zlim / bevy = {:.2}x (note: bevy has no explicit run_local)\n",
        bt.as_secs_f64() / zt.as_secs_f64()
    );
}
