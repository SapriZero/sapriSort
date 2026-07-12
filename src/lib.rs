// src/lib.rs

//! # Sapri Radix Sort Crate
//!
//! A high-performance, hybrid parallel radix sort library optimized for modern multi-core architectures.

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

// --- Module Declarations ---

/// Sequential single-core sorting engines and base core utilities.
pub mod reorder_buckets;

/// Parallel multi-core sorting engines utilizing thread pools and chunked streaming.
pub mod reorder_buckets_parallel;

/// Branchless grid network sorting engine optimized for micro-arrays.
// pub mod grid_sort;
pub mod router;

// Re-export delle funzioni e degli inizializzatori principali
pub use reorder_buckets_parallel::init_parallel_engine;
pub use router::router;

// --- Public API & Memory Arena Structure ---

/// Pre-allocated memory arena providing scratch buffers required for parallel radix sort operations.
#[derive(Debug, Default)]
pub struct SapriParallelArena {
    /// Temporary scratch buffer for primary key sorting passes (32-bit partition mapping).
    pub scratch_u32_a: Vec<u32>,
    /// Temporary secondary scratch buffer for interleaved 32-bit bucket distribution tracking.
    pub scratch_u32_b: Vec<u32>,
    /// Re-indexing linear workspace required during multi-threaded cell recombination segments.
    pub scratch_u64: Vec<u64>,
}

impl SapriParallelArena {
    /// Initializes a memory arena with a pre-allocated capacity to minimize layout shifts.
    pub fn with_capacity(capacity: usize) -> Self {
        let mut arena = Self {
            scratch_u32_a: vec![0u32; capacity],
            scratch_u32_b: vec![0u32; capacity],
            scratch_u64: vec![0u64; capacity],
        };

        if capacity > 0 {
            unsafe {
                *arena.scratch_u32_a.get_unchecked_mut(0) = 1;
                *arena.scratch_u32_a.get_unchecked_mut(capacity - 1) = 1;
                *arena.scratch_u32_b.get_unchecked_mut(0) = 1;
                *arena.scratch_u32_b.get_unchecked_mut(capacity - 1) = 1;
                *arena.scratch_u64.get_unchecked_mut(0) = 1;
                *arena.scratch_u64.get_unchecked_mut(capacity - 1) = 1;
            }
        }
        arena
    }

    /// Resizes the workspace dynamically and returns a mutable slice triad for standard execution loops.
    pub fn get_buffers_mut(&mut self, n: usize) -> (&mut [u32], &mut [u32], &mut [u64]) {
        if self.scratch_u64.len() < n { self.scratch_u64.resize(n, 0); }
        if self.scratch_u32_a.len() < n { self.scratch_u32_a.resize(n, 0); }
        if self.scratch_u32_b.len() < n { self.scratch_u32_b.resize(n, 0); }

        (
            &mut self.scratch_u32_a[0..n],
            &mut self.scratch_u32_b[0..n],
            &mut self.scratch_u64[0..n],
        )
    }
}

// --- Primary Sort Dispatchers (The Switch Layer) ---

/// Executes an unstable parallel radix sort using an external arena, automatically scanning the sequence for the maximum bound.
pub fn sapri_sort_parallel_in_arena(data: &mut [u64], arena: &mut SapriParallelArena) {
    if data.len() < 2 { return; }
    use rayon::prelude::*;
    let exact_max = data.par_iter().max().cloned().unwrap_or(0);
    sapri_sort_parallel_max(data, arena, exact_max);
}

/// Executes an unstable parallel radix sort using an explicit maximum value to completely bypass initialization scans.
pub fn sapri_sort_parallel_max(data: &mut [u64], arena: &mut SapriParallelArena, max_val: u64) {
    let n = data.len();
    if n < 2 { return; }

    if n < reorder_buckets_parallel::PARALLEL_THRESHOLD {
        router(data);
        return;
    }

    let system_threads = rayon::current_num_threads();

    if system_threads <= 1 {
        reorder_buckets::sapri_radix_core(data, max_val);
    } else {
        let (scratch_a, scratch_b, scratch_u64) = arena.get_buffers_mut(n);
        reorder_buckets_parallel::sapri_radix_parallel_core(
            data,
            max_val,
            scratch_a,
            scratch_b,
            scratch_u64,
        );
    }
}

/// Filters data elements falling within explicit boundaries into a target buffer, sorting the matched subset in-place.
pub fn sapri_filter_and_sort_parallel(
    data: &[u64],
    target_buffer: &mut [u64],
    _arena: &mut SapriParallelArena,
    min_bound: u64,
    max_bound: u64,
) -> usize {
    let n = data.len();
    if n == 0 || min_bound > max_bound { return 0; }

    let mut valid_count = 0;
    for &val in data.iter() {
        if val >= min_bound && val <= max_bound {
            target_buffer[valid_count] = val;
            valid_count += 1;
        }
    }
    
    if valid_count > 1 {
        router(&mut target_buffer[0..valid_count]);
    }

    valid_count
}

// --- Classic Library Fallbacks & Helpers ---

/// Centralized micro-routing wrapper to safely capture small workloads using branchless primitives.
#[inline(always)]
fn run_with_threshold_checks<F>(data: &mut [u64], fallback_radix_path: F)
where
    F: FnOnce(&mut [u64], usize),
{
    let n = data.len();
    if n <= 1 { return; }

    // Ripristinato l'instradamento corretto
    if n <= 32 {
        router(data);
        return;
    }

    if n < reorder_buckets_parallel::PARALLEL_THRESHOLD {
        data.sort_unstable();
        return;
    }

    fallback_radix_path(data, n);
}

/// Performs an unstable hybrid sort, isolating tiny datasets inside the grid engine.
pub fn sapri_sort_unstable(data: &mut [u64]) {
    run_with_threshold_checks(data, |slice, n| {
        let mut arena = SapriParallelArena::with_capacity(n);
        sapri_sort_parallel_in_arena(slice, &mut arena);
    });
}

/// Performs an unstable hybrid sort with a predefined maximum value limit.
pub fn sapri_sort_unstable_max(data: &mut [u64], max_val: u64) {
    run_with_threshold_checks(data, |slice, n| {
        let mut arena = SapriParallelArena::with_capacity(n);
        sapri_sort_parallel_max(slice, &mut arena, max_val);
    });
}

/// Performs an unstable hybrid sort with explicit lower and upper boundaries.
pub fn sapri_sort_unstable_min_max(data: &mut [u64], min_val: u64, max_val: u64) {
    let _ = min_val; 
    sapri_sort_unstable_max(data, max_val);
}

/// Performs a stable sort operation fallback on matching elements.
pub fn sapri_sort_stable(data: &mut [u64]) {
    let n = data.len();
    if n <= 1 { return; }
    if n <= 1024 {
        (data);
        return;
    }
    data.sort(); 
}

// --- High-Level Shorthand Aliases (Exposed Frontend) ---

/// Standard exposed shorthand wrapper executing unstable parallel sorts.
#[inline(always)]
pub fn sapri_sort(data: &mut [u64]) {
    sapri_sort_unstable(data);
}

/// Standard exposed shorthand wrapper executing bounded max sorts.
#[inline(always)]
pub fn sapri_sort_max(data: &mut [u64], max_val: u64) {
    sapri_sort_unstable_max(data, max_val);
}

/// Standard exposed shorthand wrapper executing bounded min-max range sorts.
#[inline(always)]
pub fn sapri_sort_min_max(data: &mut [u64], min_val: u64, max_val: u64) {
    sapri_sort_unstable_min_max(data, min_val, max_val);
}
