//! Multi-threaded benchmark: zlim-task vs bevy_tasks.
//!
//! Run with `cargo run --release`.

use rand::seq::SliceRandom;
use std::time::Instant;

const THREADS: usize = 10;
const TASKS: usize = 100_000;
const WARMUP: usize = 3;
const RUNS: usize = 5;

fn main() {
    println!(
        "multi-threaded — {THREADS} threads, {TASKS} tasks, {RUNS} runs (after {WARMUP} warmups)\n"
    );

    bench_scope_local();
    bench_scope_tiny();
    bench_scope_heavy();
    bench_par_map();
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
// tiny task — measures scheduling overhead
// -----------------------------------------------------------------------------

fn bench_scope_local() {
    println!("--- scope spawn_local (tiny task) ---");

    let zt = {
        let zp = zlim_task::TaskPoolBuilder::new()
            .thread_count(THREADS)
            .build();
        time("zlim-task", || {
            zp.scope(|s| {
                for i in 0..TASKS {
                    s.spawn_local(async move { i });
                }
            })
        })
    };

    let bt = {
        let bp = bevy_tasks::TaskPoolBuilder::new()
            .num_threads(THREADS)
            .build();
        time("bevy_tasks", || {
            bp.scope(|s| {
                for i in 0..TASKS {
                    s.spawn_on_scope(async move { i });
                }
            })
        })
    };

    println!(
        "  → zlim / bevy = {:.2}x\n",
        bt.as_secs_f64() / zt.as_secs_f64()
    );
}

fn bench_scope_tiny() {
    println!("--- scope spawn (tiny task) ---");

    let zt = {
        let zp = zlim_task::TaskPoolBuilder::new()
            .thread_count(THREADS)
            .build();
        time("zlim-task", || {
            zp.scope(|s| {
                for i in 0..TASKS {
                    s.spawn(async move { i });
                }
            })
        })
    };

    let bt = {
        let bp = bevy_tasks::TaskPoolBuilder::new()
            .num_threads(THREADS)
            .build();
        time("bevy_tasks", || {
            bp.scope(|s| {
                for i in 0..TASKS {
                    s.spawn(async move { i });
                }
            })
        })
    };

    println!(
        "  → zlim / bevy = {:.2}x\n",
        bt.as_secs_f64() / zt.as_secs_f64()
    );
}

// -----------------------------------------------------------------------------
// heavier task — simulates a small amount of work per task
// -----------------------------------------------------------------------------

fn bench_scope_heavy() {
    println!("--- scope spawn (compute) ---");

    let zt = {
        let zp = zlim_task::TaskPoolBuilder::new()
            .thread_count(THREADS)
            .build();
        time("zlim-task", || {
            zp.scope(|s| {
                for i in 0..TASKS {
                    s.spawn(async move { (0..100).fold(i, |a, b| a ^ b) });
                }
            })
        })
    };

    let bt = {
        let bp = bevy_tasks::TaskPoolBuilder::new()
            .num_threads(THREADS)
            .build();
        time("bevy_tasks", || {
            bp.scope(|s| {
                for i in 0..TASKS {
                    s.spawn(async move { (0..100).fold(i, |a, b| a ^ b) });
                }
            })
        })
    };

    println!(
        "  → zlim / bevy = {:.2}x\n",
        bt.as_secs_f64() / zt.as_secs_f64()
    );
}

// -----------------------------------------------------------------------------
// parallel slice work-alike
// -----------------------------------------------------------------------------

#[inline(never)]
fn hash_slice(slice: &[u32]) -> u64 {
    let mut acc = 0u64;
    for &v in slice {
        let mut h: u64 = v as u64;
        for _ in 0..10_000 {
            h = h.wrapping_mul(0x01000193).wrapping_add(h >> 13);
        }
        acc ^= h;
    }
    core::hint::black_box(acc)
}

fn bench_par_map() {
    const CHUNKS: usize = 200;

    let data: Vec<u32> = {
        let mut v: Vec<u32> = (0..CHUNKS as u32 * 256).collect();
        v.shuffle(&mut rand::rng());
        v
    };

    println!("--- parallel map ({CHUNKS} chunks) ---");

    let zt = {
        let zp = zlim_task::TaskPoolBuilder::new()
            .thread_count(THREADS)
            .build();
        time("zlim-task scope", || {
            zp.scope(|s| {
                for chunk in data.chunks(data.len() / CHUNKS) {
                    s.spawn(async move { hash_slice(chunk) });
                }
            })
        })
    };

    let bt = {
        let bp = bevy_tasks::TaskPoolBuilder::new()
            .num_threads(THREADS)
            .build();
        time("bevy_tasks scope", || {
            bp.scope(|s| {
                for chunk in data.chunks(data.len() / CHUNKS) {
                    s.spawn(async move { hash_slice(chunk) });
                }
            })
        })
    };

    // In the current design, when the main thread runs a `Scope` it pulls
    // tasks from the global queue but does *not* steal from worker threads.
    // This is intentional — the main thread has its own `MainExecutor`
    // workload and should not be burdened with worker over-spill.  In a
    // flat parallel benchmark this effectively means one fewer worker, so
    // zlim is expected to be slightly slower than bevy here.
    println!(
        "  → zlim / bevy = {:.2}x\n",
        bt.as_secs_f64() / zt.as_secs_f64()
    );
}
