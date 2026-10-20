// src/lib.rs

//! # Sapri Radix Sort Crate
//!
//! A high-performance, hybrid parallel radix sort library optimized for modern multi-core architectures,
//! featuring dynamic domain routing, hardware-aware concurrency thresholds, and specialized low-bit counting sorts.

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

// --- Module Declarations ---

/// Sequential single-core sorting engines and base core utilities.
pub mod serial;

/// Parallel multi-core sorting engines utilizing thread pools and chunked streaming.
pub mod parallel;

/// Dynamic sorting strategy router and hardware concurrency dispatcher.
pub mod router;

/// Low-level memory scatter kernels and hardware prefetching helpers.
pub mod scatter;

/// Public high-level API frontend and shorthand sorting wrappers.
pub mod api;

/// Specialized parallel counting sort engines for bounded 8-bit and 16-bit datasets.
pub mod specialized_parallel;

/// Specialized serial counting sort engines for bounded 8-bit and 16-bit datasets.
pub mod specialized_serial;

// --- Re-exports and Thread Pool Initialization ---

pub use parallel::init_parallel_engine;
pub use router::sapri_sort;
pub use api::*;

use std::cell::RefCell;
use thread_local::ThreadLocal;

pub use specialized_parallel::{sapri_sort_8, sapri_sort_16};
pub use specialized_serial::{sapri_sort_8_serial, sapri_sort_16_serial};

/// Placeholder configuration structure for bit topology selection.
#[derive(Debug, Clone, Copy)]
pub enum BitTopology {
    /// Automatically adapts the bit-width extraction pass based on value distribution.
    Adaptive,
}

/// Placeholder scratchpad structure for thread-local workspace reuse.
#[derive(Debug, Default)]
pub struct ThreadScratchpad {
    /// Internal vector workspace for temporary data staging.
    pub workspace: Vec<u64>,
}

lazy_static::lazy_static! {
    /// Global persistent thread-local storage pooling scratchpads across thread counts.
    pub static ref GLOBAL_SCRATCH_POOL: ThreadLocal<RefCell<ThreadScratchpad>> = ThreadLocal::new();
}

// --- Memory Arena Structure ---

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

// --- Primary Sort Dispatchers ---

/// Executes an unstable parallel radix sort using an external arena, automatically scanning the sequence for the maximum bound.
pub fn sapri_sort_parallel_in_arena(data: &mut [u64], arena: &mut SapriParallelArena, topology: BitTopology) {
    if data.len() < 2 { return; }
    let exact_max = data.iter().max().copied().unwrap_or(0);
    sapri_sort_parallel_max(data, arena, exact_max, topology);
}

/// Executes an unstable parallel radix sort using an explicit maximum value to completely bypass initialization scans.
pub fn sapri_sort_parallel_max(data: &mut [u64], arena: &mut SapriParallelArena, max_val: u64, _topology: BitTopology) {
    let n = data.len();
    if n < 2 { return; }

    let system_threads = rayon::current_num_threads();

    if system_threads <= 1 {
        serial::sapri_radix_core(data, max_val);
    } else {
        let (scratch_a, scratch_b, scratch_u64) = arena.get_buffers_mut(n);
        parallel::sapri_radix_parallel_core(
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
    _topology: BitTopology,
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
        sapri_sort(&mut target_buffer[0..valid_count]);
    }

    valid_count
}
