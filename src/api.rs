// src/api.rs

//! # Public API Frontend
//!
//! Provides user-facing sorting shorthands, safety wrappers, specialized low-bit counting sort dispatches,
//! and signed type extensions (`i64`), bridging high-level ergonomics with internal performance engines.

use crate::BitTopology;
use crate::parallel;
use crate::specialized_parallel::{sapri_sort_8, sapri_sort_16};

/// Centralized micro-routing wrapper to safely capture small workloads and dispatch
/// to specialized low-bit counting sorts or general radix engines based on data domain bounds.
#[inline(always)]
fn run_with_threshold_checks<F>(data: &mut [u64], max_val: u64, _topology: BitTopology, fallback_radix_path: F)
where
    F: FnOnce(&mut [u64], usize),
{
    let n = data.len();
    if n <= 1 { return; }

    // Fast-path for micro-arrays to eliminate dispatch/allocation overhead
    if n <= 32 {
        data.sort_unstable();
        return;
    }

    // 1. Specialized 8-bit Counting Sort Dispatch ($O(N)$ domain restriction & threshold guard)
    if max_val <= 255 && n >= 2000 {
        sapri_sort_8(data);
        return;
    }

    // 2. Specialized 16-bit Counting Sort Dispatch ($O(N)$ domain restriction & threshold guard)
    if max_val <= 65535 && n >= 5000 {
        sapri_sort_16(data);
        return;
    }

    if n < parallel::PARALLEL_THRESHOLD {
        data.sort_unstable();
        return;
    }

    fallback_radix_path(data, n);
}

/// Performs an unstable hybrid sort, inspecting maximum value bounds on the fly.
pub fn sapri_sort_unstable(data: &mut [u64], topology: BitTopology) {
    let exact_max = data.iter().max().copied().unwrap_or(0);
    run_with_threshold_checks(data, exact_max, topology, |slice, n| {
        let mut arena = crate::SapriParallelArena::with_capacity(n);
        crate::sapri_sort_parallel_in_arena(slice, &mut arena, topology);
    });
}

/// Performs an unstable hybrid sort with a predefined maximum value limit.
pub fn sapri_sort_unstable_max(data: &mut [u64], max_val: u64, topology: BitTopology) {
    run_with_threshold_checks(data, max_val, topology, |slice, n| {
        let mut arena = crate::SapriParallelArena::with_capacity(n);
        crate::sapri_sort_parallel_max(slice, &mut arena, max_val, topology);
    });
}

/// Performs an unstable hybrid sort with explicit lower and upper boundaries.
pub fn sapri_sort_unstable_min_max(data: &mut [u64], min_val: u64, max_val: u64, topology: BitTopology) {
    let _ = min_val; 
    sapri_sort_unstable_max(data, max_val, topology);
}

// --- High-Level Shorthand Aliases ---

/// Standard exposed shorthand wrapper executing unstable parallel sorts with default adaptive bit topology.
#[inline(always)]
pub fn sapri_sort(data: &mut [u64]) {
    sapri_sort_unstable(data, BitTopology::Adaptive);
}

/// Standard exposed shorthand wrapper executing bounded max sorts with default adaptive bit topology.
#[inline(always)]
pub fn sapri_sort_max(data: &mut [u64], max_val: u64) {
    sapri_sort_unstable_max(data, max_val, BitTopology::Adaptive);
}

/// Standard exposed shorthand wrapper executing bounded min-max range sorts with default adaptive bit topology.
#[inline(always)]
pub fn sapri_sort_min_max(data: &mut [u64], min_val: u64, max_val: u64) {
    sapri_sort_unstable_min_max(data, min_val, max_val, BitTopology::Adaptive);
}

/// Standard exposed shorthand wrapper executing unstable parallel sorts with an explicit bit topology.
#[inline(always)]
pub fn sapri_sort_topo(data: &mut [u64], topology: BitTopology) {
    sapri_sort_unstable(data, topology);
}

/// Standard exposed shorthand wrapper executing bounded max sorts with an explicit bit topology.
#[inline(always)]
pub fn sapri_sort_max_topo(data: &mut [u64], max_val: u64, topology: BitTopology) {
    sapri_sort_unstable_max(data, max_val, topology);
}

/// Standard exposed shorthand wrapper executing bounded min-max range sorts with an explicit bit topology.
#[inline(always)]
pub fn sapri_sort_min_max_topo(data: &mut [u64], min_val: u64, max_val: u64, topology: BitTopology) {
    sapri_sort_unstable_min_max(data, min_val, max_val, topology);
}

// --- High-Level Signed i64 Extensions ---

/// Executes an unstable adaptive sort over a signed `i64` dataset.
#[inline(always)]
pub fn sapri_sort_i64(data: &mut [i64]) {
    sapri_sort_i64_topo(data, BitTopology::Adaptive);
}

/// Executes an adaptive sort over a signed `i64` dataset using an explicit bit topology.
pub fn sapri_sort_i64_topo(data: &mut [i64], topology: BitTopology) {
    sapri_sort_i64_range(data, i64::MIN, i64::MAX, topology);
}

/// Executes an optimized sort over a signed `i64` dataset with explicit Min and Max bounds via bias mapping.
pub fn sapri_sort_i64_range(data: &mut [i64], min_val: i64, max_val: i64, topology: BitTopology) {
    let n = data.len();
    let _ = (min_val, max_val);
    
    if n < 32 || n > 5_000_000 {
        data.sort_unstable();
        return;
    }

    let u_data: &mut [u64] = unsafe {
        std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u64, n)
    };

    for val in u_data.iter_mut() {
        *val = (*val as i64).wrapping_add(i64::MIN) as u64;
    }

    if n < parallel::PARALLEL_THRESHOLD {
        u_data.sort_unstable();
    } else {
        let mut arena = crate::SapriParallelArena::with_capacity(n);
        crate::sapri_sort_parallel_in_arena(u_data, &mut arena, topology);
    }

    for val in u_data.iter_mut() {
        *val = (*val as u64).wrapping_sub(i64::MIN as u64);
    }
}
