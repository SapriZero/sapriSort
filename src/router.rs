// src/router.rs

//! # Dynamic Sorting Strategy Router
//!
//! This module provides central routing intelligence for the `SAPRI` sorting library.
//! It evaluates dataset cardinality, numerical value bounds, and hardware concurrency 
//! at runtime to dynamically dispatch sorting tasks to the most optimal underlying engine.

use crate::specialized_parallel::{sapri_sort_8, sapri_sort_16};

/// Dynamically analyzes a slice of 64-bit unsigned integers and dispatches 
/// the most efficient sorting strategy based on size, domain distribution, and active CPU cores.
///
/// # Dispatch Heuristics:
/// - **Small & Medium Workloads ($N < 2,000,000$):** Routes directly to Rust's native unstable 
///   comparison sort (`sort_unstable`), eliminating any allocation or dispatch overhead.
/// - **Single-Core Environments ($Cores \le 1$):** Bypasses parallel engines entirely to prevent 
///   multithreading synchronization overhead.
/// - **8-bit Bounded Domain ($\max \le 255$):** Dispatches to the specialized $O(N)$ counting sort engine.
/// - **16-bit Bounded Domain ($\max \le 65535$):** Dispatches to the optimized 16-bit specialized engine.
/// - **Massive Scale Unbounded ($N \ge 2,000,000$):** Unleashes the multi-core Rayon-based parallel radix sort.
#[inline]
pub fn sapri_sort(data: &mut [u64]) {
    let n = data.len();
    if n < 2 {
        return;
    }

    // --- 1. Fast-Path Threshold Guard ---
    // For datasets below 2 million elements, standard comparison sort delivers superior performance
    // and avoids the coordination overhead of radix or parallel execution pipelines.
    if n < 2_000_000 {
        data.sort_unstable();
        return;
    }

    // Retrieve active core count from the Rayon execution pool
    let num_cores = rayon::current_num_threads();

    // Fall back to native unstable sort if running on a single-core or restricted execution context
    if num_cores <= 1 {
        data.sort_unstable();
        return;
    }

    // --- 2. Value Domain Inspection ---
    // Rapid linear scan to evaluate the upper bound of the dataset (auto-vectorized by LLVM)
    let mut max_val = 0u64;
    for &val in data.iter() {
        if val > max_val {
            max_val = val;
        }
    }

    // Specialized 8-bit Counting Sort Dispatch ($O(N)$ domain restriction)
    if max_val <= 255 {
        sapri_sort_8(data);
        return;
    }

    // Specialized 16-bit Counting Sort Dispatch ($O(N)$ domain restriction)
    if max_val <= 65535 {
        sapri_sort_16(data);
        return;
    }

    // --- 3. Massive Scale Parallel Radix Dispatch ---
    let mut arena = crate::SapriParallelArena::with_capacity(n);
    crate::sapri_sort_parallel_in_arena(data, &mut arena, crate::BitTopology::Adaptive);
}
