// src/lib.rs

//! # Sapri Radix Sort Crate
//!
//! A high-performance, hybrid parallel radix sort library built for extreme performance.
//! It features multi-core sorting with Rayon, zero-allocation memory reuse via an external arena,
//! and hardware-level prefetching optimizations for L1/L2 cache efficiency.
//!
//! This crate is structured to separate single-core sequential algorithms from parallel 
//! engines while sharing atomic sub-operations to ensure maximum code reuse and maintainability.

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

// --- Module Declarations ---

/// Sequential single-core sorting engines and base core utilities.
pub mod reorder_buckets;

/// Parallel multi-core sorting engines utilizing thread pools and chunked streaming.
pub mod reorder_buckets_parallel;

// --- Public API & Memory Arena Structure ---

/// An external memory buffer pool designed to completely eliminate allocations 
/// during continuous execution of parallel radix sort passes.
///
/// It pre-allocates and buffers `u32` and `u64` slices aligned with the input dataset size `n`,
/// acting as a high-speed scratch space for both downcast and full 64-bit passes.
#[derive(Debug, Default)]
pub struct SapriParallelArena {
    scratch_u32_a: Vec<u32>,
    scratch_u32_b: Vec<u32>,
    scratch_u64: Vec<u64>,
}

impl SapriParallelArena {
    /// Creates a new, empty parallel memory arena.
    #[inline]
    pub fn new() -> Self {
        Self {
            scratch_u32_a: Vec::new(),
            scratch_u32_b: Vec::new(),
            scratch_u64: Vec::new(),
        }
    }

    /// Pre-allocates or resizes internal storage to efficiently handle a dataset of size `n`.
    /// 
    /// Calling this before a sorting loop prevents dynamic reallocation penalties at runtime.
    pub fn reserve(&mut self, n: usize) {
        if self.scratch_u32_a.len() < n {
            self.scratch_u32_a.resize(n, 0);
        }
        if self.scratch_u32_b.len() < n {
            self.scratch_u32_b.resize(n, 0);
        }
        if self.scratch_u64.len() < n {
            self.scratch_u64.resize(n, 0);
        }
    }

    /// Borrows mutable disjoint buffers tailored for the parallel engine processing `n` elements.
    ///
    /// # Arguments
    /// * `n` - The exact number of elements currently being processed.
    ///
    /// # Returns
    /// A tuple containing `(&mut [u32], &mut [u32], &mut [u64])` matched precisely to size `n`.
    #[inline]
    pub fn get_buffers_mut(&mut self, n: usize) -> (&mut [u32], &mut [u32], &mut [u64]) {
        // Enforce size compliance seamlessly
        self.reserve(n);

        (
            &mut self.scratch_u32_a[..n],
            &mut self.scratch_u32_b[..n],
            &mut self.scratch_u64[..n],
        )
    }

    /// Clears the underlying vectors and returns reclaimed memory back to the operating system.
    pub fn shrink_to_fit(&mut self) {
        self.scratch_u32_a.shrink_to_fit();
        self.scratch_u32_b.shrink_to_fit();
        self.scratch_u64.shrink_to_fit();
    }
}
