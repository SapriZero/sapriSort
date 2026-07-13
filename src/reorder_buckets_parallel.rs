// src/reorder_buckets_parallel.rs

//! # Parallel Hybrid Radix Sort Engine
//!
//! Provides multi-core parallel sorting routines powered by Rayon.
//! This module uses chunked data streaming, thread-local concurrent histogram accumulation,
//! and parallel scatter operations optimized with x86_64 L1 cache-line prefetching.

use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
use crate::SapriParallelArena;

// Atomic shared utility functions imported from the sequential module
use crate::reorder_buckets::check_can_skip_pass;

/// The element capacity threshold per parallel thread processing chunk.
const CHUNK_SIZE: usize = 128 * 1024;

/// The minimum number of elements required to trigger the parallel execution engine.
pub const PARALLEL_THRESHOLD: usize = 2_000_000; 

/// Initializes the global multi-threaded Rayon worker pool with a designated core count.
///
/// # Arguments
/// * `num_cores` - The explicit number of threads to assign. If set to `0`, it relies 
///   on standard OS environment constraints or automatic physical core counts.
pub fn init_parallel_engine(num_cores: usize) {
    let mut builder = ThreadPoolBuilder::new();
    if num_cores > 0 { builder = builder.num_threads(num_cores); }
    let _ = builder.build_global();
}

/// Main high-performance hybrid core dispatcher.
///
/// Routes execution branches toward the 32-bit compressed parallel pipeline or 
/// fallback native 64-bit multi-threaded distribution engines based on `max_val`.
pub fn sapri_radix_parallel_core(
    data: &mut [u64], 
    max_val: u64,
    scratch_u32_a: &mut [u32],
    scratch_u32_b: &mut [u32],
    scratch_u64: &mut [u64],
) {
    let n = data.len();
    if n < 2 { return; }

    if n < PARALLEL_THRESHOLD {
        data.sort_unstable();
        return;
    }

    let bits = if max_val == 0 { 1 } else { 64 - max_val.leading_zeros() as usize };
    let passes = (bits + 7) / 8;

    if max_val <= u32::MAX as u64 {
        sapri_radix_32bit_parallel_engine(data, passes, scratch_u32_a, scratch_u32_b, scratch_u64);
    } else {
        sapri_radix_64bit_parallel_engine(data, passes, scratch_u64);
    }
}

// =========================================================================
// DATA STRUCTURES AND CONFIGURATION ENUMS
// =========================================================================

/// Tracks the active hot memory zone location for 32-bit alternate radix streaming passes.
#[derive(Debug, PartialEq, Clone, Copy)]
enum DataLocation {
    /// The original input vector data segment.
    OriginalData,
    /// The primary intermediate 32-bit workspace buffer.
    ScratchA,
    /// The secondary intermediate 32-bit alternate workspace buffer.
    ScratchB,
}

/// Holds thread-local execution boundary metrics to perform deterministic concurrent memory scattering.
pub struct ScatterMap {
    /// Disjoint layout mappings matching thread matrices over 256 individual radix buckets.
    thread_offsets: Vec<[usize; 256]>,
}

// =========================================================================
// 32-BIT PARALLEL ENGINE
// =========================================================================

/// Specialized compressed parallel 32-bit hybrid distribution execution loop.
fn sapri_radix_32bit_parallel_engine(
    data: &mut [u64], 
    passes: usize, 
    scratch_a: &mut [u32], 
    scratch_b: &mut [u32],
    scratch_u64: &mut [u64],
) {
    let n = data.len();
    if n == 0 { return; }

    let mut data_location = DataLocation::OriginalData;
    let mut src_ptr = scratch_a.as_mut_ptr();
    let mut dst_ptr = scratch_b.as_mut_ptr();

    for i in 0..passes {
        let shift = (i * 8) as u32;
        let is_last = i == passes - 1;

        if data_location == DataLocation::OriginalData {
            data_location = execute_u64_parallel_pass(data, scratch_a, scratch_u64, shift, is_last, n);
            if data_location == DataLocation::OriginalData && !is_last {
                // Sorting pass omitted safely due to missing bit variances
                continue;
            }
            src_ptr = scratch_a.as_mut_ptr();
            dst_ptr = scratch_b.as_mut_ptr();
        } else {
            let current_src = unsafe { std::slice::from_raw_parts(src_ptr, n) };
            let (next_loc, next_src, next_dst) = execute_scratch_parallel_pass(
                current_src, scratch_a, scratch_b, data, src_ptr, dst_ptr, data_location, shift, is_last, n
            );
            data_location = next_loc;
            src_ptr = next_src;
            dst_ptr = next_dst;
        }
    }
}

/// Executes a parallel processing pass from the 64-bit root data stream.
#[inline(always)]
fn execute_u64_parallel_pass(
    data: &mut [u64],
    scratch_a: &mut [u32],
    scratch_u64: &mut [u64],
    shift: u32,
    is_last: bool,
    n: usize,
) -> DataLocation {
    let hists = phase_1_parallel_u64(data, shift);
    
    if check_can_skip_pass(&hists, n) {
        return DataLocation::OriginalData;
    }

    let smap = compute_scatter_map(&hists);
    if is_last {
        scatter_parallel_u64_to_u64(data, &smap, scratch_u64, shift);
        data.copy_from_slice(scratch_u64);
        DataLocation::OriginalData
    } else {
        scatter_parallel_u64_to_u32(data, &smap, scratch_a, shift);
        DataLocation::ScratchA
    }
}

/// Executes a parallel processing pass inside the downcasted 32-bit workspace buffers.
#[inline(always)]
fn execute_scratch_parallel_pass(
    current_src: &[u32],
    scratch_a: &mut [u32],
    scratch_b: &mut [u32],
    data: &mut [u64],
    src_ptr: *mut u32,
    dst_ptr: *mut u32,
    current_loc: DataLocation,
    shift: u32,
    is_last: bool,
    n: usize,
) -> (DataLocation, *mut u32, *mut u32) {
    let hists = phase_1_parallel_u32(current_src, shift);
    
    if check_can_skip_pass(&hists, n) {
        if is_last {
            if current_loc == DataLocation::ScratchA { linear_upcast_u32_to_u64(scratch_a, data); } 
            else { linear_upcast_u32_to_u64(scratch_b, data); }
            return (DataLocation::OriginalData, src_ptr, dst_ptr);
        }
        return (current_loc, src_ptr, dst_ptr);
    }

    let smap = compute_scatter_map(&hists);
    if is_last {
        scatter_parallel_u32_to_64(current_src, &smap, data, shift);
        (DataLocation::OriginalData, src_ptr, dst_ptr)
    } else {
        let current_dst = unsafe { std::slice::from_raw_parts_mut(dst_ptr, n) };
        scatter_parallel_u32_to_u32(current_src, &smap, current_dst, shift);
        
        let next_loc = if current_loc == DataLocation::ScratchA { DataLocation::ScratchB } else { DataLocation::ScratchA };
        (next_loc, dst_ptr, src_ptr) // Zero-overhead atomic raw pointer swap
    }
}

// =========================================================================
// 64-BIT PARALLEL ENGINE
// =========================================================================

/// Native fallback multi-threaded 64-bit sorting engine layout.
fn sapri_radix_64bit_parallel_engine(data: &mut [u64], passes: usize, scratch: &mut [u64]) {
    let n = data.len();
    let mut src_is_data = true;

    for i in 0..passes {
        let shift = (i * 8) as u32;
        
        src_is_data = if src_is_data {
            execute_64bit_pass_step(data, scratch, shift, n)
        } else {
            !execute_64bit_pass_step(scratch, data, shift, n)
        };
    }

    if !src_is_data { data.copy_from_slice(scratch); }
}

/// Implements a discrete 64-bit thread distribution loop block step.
#[inline(always)]
fn execute_64bit_pass_step(src: &mut [u64], dst: &mut [u64], shift: u32, n: usize) -> bool {
    let hists = phase_1_parallel_u64(src, shift);
    if check_can_skip_pass(&hists, n) {
        dst.copy_from_slice(src);
        return false;
    }
    let smap = compute_scatter_map(&hists);
    scatter_parallel_u64_to_u64(src, &smap, dst, shift);
    false
}

// =========================================================================
// HISTOGRAMMING AND PARALLEL SCATTER MAP LOGIC
// =========================================================================

/// Concurrently processes thread-local byte histograms from 64-bit arrays.
#[inline(always)]
fn phase_1_parallel_u64(data: &[u64], shift: u32) -> Vec<[usize; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut local_hist = [0usize; 256];
            for &val in chunk { local_hist[((val >> shift) & 0xFF) as usize] += 1; }
            local_hist
        })
        .collect()
}

/// Concurrently processes thread-local byte histograms from 32-bit arrays.
#[inline(always)]
fn phase_1_parallel_u32(data: &[u32], shift: u32) -> Vec<[usize; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut local_hist = [0usize; 256];
            for &val in chunk { local_hist[((val >> shift) & 0xFF) as usize] += 1; }
            local_hist
        })
        .collect()
}

/// Accumulates local matrices to resolve global distribution scatter mappings across worker fields.
fn compute_scatter_map(local_hists: &[[usize; 256]]) -> ScatterMap {
    let num_threads = local_hists.len();
    let mut totals = [0usize; 256];
    
    for hist in local_hists {
        for b in 0..256 { totals[b] += hist[b]; }
    }

    let mut global_offsets = [0usize; 256];
    let mut running = 0usize;
    for b in 0..256 {
        global_offsets[b] = running;
        running += totals[b];
    }

    let mut thread_offsets = vec![[0usize; 256]; num_threads];
    for b in 0..256 {
        let mut cursor = global_offsets[b];
        for t in 0..num_threads {
            thread_offsets[t][b] = cursor;
            cursor += local_hists[t][b];
        }
    }

    ScatterMap { thread_offsets }
}

// =========================================================================
// CACHE-LINE PREFETCH PARALLEL SCATTER LOOPS
// =========================================================================

/// Scatters elements concurrently from a 64-bit array down to a 32-bit workspace using lookahead prefetching.
fn scatter_parallel_u64_to_u32(src: &[u64], smap: &ScatterMap, dst: &mut [u32], shift: u32) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let dst_len = dst.len();
    let thread_offsets = &smap.thread_offsets;

    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(t, chunk)| {
        let mut ptrs = thread_offsets[t];
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u32, dst_len) };
        let len = chunk.len();

        for i in 0..len {
            if i + 8 < len { // Prefetching optimization: 8 elements * 64 bits = 64-byte L1 Cache Line
                unsafe {
                    let lookahead_val = *chunk.get_unchecked(i + 8);
                    let lookahead_b = ((lookahead_val >> shift) & 0xFF) as usize;
                    let lookahead_pos = *ptrs.get_unchecked(lookahead_b);
                    #[cfg(target_arch = "x86_64")]
                    _mm_prefetch(out.as_ptr().add(lookahead_pos) as *const i8, _MM_HINT_T0);
                }
            }
            unsafe {
                let val = *chunk.get_unchecked(i);
                let b = ((val >> shift) & 0xFF) as usize;
                let pos = ptrs[b];
                *out.get_unchecked_mut(pos) = val as u32;
                *ptrs.get_unchecked_mut(b) = pos + 1;
            }
        }
    });
}

/// Scatters elements concurrently between independent 32-bit workspaces using lookahead prefetching.
fn scatter_parallel_u32_to_u32(src: &[u32], smap: &ScatterMap, dst: &mut [u32], shift: u32) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let dst_len = dst.len();
    let thread_offsets = &smap.thread_offsets;

    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(t, chunk)| {
        let mut ptrs = thread_offsets[t];
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u32, dst_len) };
        let len = chunk.len();

        for i in 0..len {
            if i + 16 < len { // Prefetching optimization: 16 elements * 32 bits = 64-byte L1 Cache Line
                unsafe {
                    let lookahead_val = *chunk.get_unchecked(i + 16);
                    let lookahead_b = ((lookahead_val >> shift) & 0xFF) as usize;
                    let lookahead_pos = *ptrs.get_unchecked(lookahead_b);
                    #[cfg(target_arch = "x86_64")]
                    _mm_prefetch(out.as_ptr().add(lookahead_pos) as *const i8, _MM_HINT_T0);
                }
            }
            unsafe {
                let val = *chunk.get_unchecked(i);
                let b = ((val >> shift) & 0xFF) as usize;
                let pos = ptrs[b];
                *out.get_unchecked_mut(pos) = val;
                *ptrs.get_unchecked_mut(b) = pos + 1;
            }
        }
    });
}

/// Scatters elements concurrently from a 32-bit workspace up to a 64-bit target stream using lookahead prefetching.
fn scatter_parallel_u32_to_64(src: &[u32], smap: &ScatterMap, dst: &mut [u64], shift: u32) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let dst_len = dst.len();
    let thread_offsets = &smap.thread_offsets;

    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(t, chunk)| {
        let mut ptrs = thread_offsets[t];
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u64, dst_len) };
        let len = chunk.len();

        for i in 0..len {
            if i + 16 < len {
                unsafe {
                    let lookahead_val = *chunk.get_unchecked(i + 16);
                    let lookahead_b = ((lookahead_val >> shift) & 0xFF) as usize;
                    let lookahead_pos = *ptrs.get_unchecked(lookahead_b);
                    #[cfg(target_arch = "x86_64")]
                    _mm_prefetch(out.as_ptr().add(lookahead_pos) as *const i8, _MM_HINT_T0);
                }
            }
            unsafe {
                let val = *chunk.get_unchecked(i);
                let b = ((val >> shift) & 0xFF) as usize;
                let pos = ptrs[b];
                *out.get_unchecked_mut(pos) = val as u64;
                *ptrs.get_unchecked_mut(b) = pos + 1;
            }
        }
    });
}

/// Scatters elements concurrently between independent 64-bit segments using lookahead prefetching.
fn scatter_parallel_u64_to_u64(src: &[u64], smap: &ScatterMap, dst: &mut [u64], shift: u32) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let dst_len = dst.len();
    let thread_offsets = &smap.thread_offsets;

    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(t, chunk)| {
        let mut ptrs = thread_offsets[t];
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u64, dst_len) };
        let len = chunk.len();

        for i in 0..len {
            if i + 8 < len {
                unsafe {
                    let lookahead_val = *chunk.get_unchecked(i + 8);
                    let lookahead_b = ((lookahead_val >> shift) & 0xFF) as usize;
                    let lookahead_pos = *ptrs.get_unchecked(lookahead_b);
                    #[cfg(target_arch = "x86_64")]
                    _mm_prefetch(out.as_ptr().add(lookahead_pos) as *const i8, _MM_HINT_T0);
                }
            }
            unsafe {
                let val = *chunk.get_unchecked(i);
                let b = ((val >> shift) & 0xFF) as usize;
                let pos = ptrs[b];
                *out.get_unchecked_mut(pos) = val;
                *ptrs.get_unchecked_mut(b) = pos + 1;
            }
        }
    });
}

// =========================================================================
// ARENA INTEGRATION AND LINEAR WORKSPACE CASTS
// =========================================================================

/// **PARALLEL SORT DRIVER**: High-speed entry point leveraging an external arena workspace.
///
/// Deterministically measures global data distribution bounds using precise full-scan parallel
/// aggregation to adjust processing radix steps and enforce zero runtime heap allocations.
pub fn sapri_sort_parallel_in_arena(data: &mut [u64], _passes: usize, arena: &mut SapriParallelArena) {
    let n = data.len();
    if n < 2 { return; }

    // Accurate high-performance parallel min/max reduction to resolve true data range
    let (global_min, global_max) = data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut l_min = chunk[0];
            let mut l_max = chunk[0];
            for &v in chunk.iter().skip(1) {
                if v < l_min { l_min = v; }
                if v > l_max { l_max = v; }
            }
            (l_min, l_max)
        })
        .reduce(
            || (u64::MAX, 0u64),
            |(a_min, a_max), (b_min, b_max)| (a_min.min(b_min), a_max.max(b_max))
        );

    if global_min == global_max { return; }
    let estimated_range = global_max.saturating_sub(global_min);

    let required_bits = 64 - estimated_range.leading_zeros() as usize;
    let dynamic_threshold = match required_bits {
        0..=8   => 800_000,   
        9..=16  => 1_800_000, 
        17..=24 => 3_000_000, 
        _       => 4_500_000, 
    };

    if n < dynamic_threshold {
        data.sort_unstable();
        return;
    }

    if (n as u64) / estimated_range > 500 {
        data.sort_unstable();
        return;
    }

    // Compute effective radix sorting passes required by the real bit width
    let actual_passes = (required_bits + 7) / 8;
    let (scratch_a, scratch_b, scratch_u64) = arena.get_buffers_mut(n);

    // Dynamic state routing based on global upper boundaries
    if global_max <= u32::MAX as u64 {
        sapri_radix_32bit_parallel_engine(data, actual_passes, scratch_a, scratch_b, scratch_u64);
    } else {
        sapri_radix_64bit_parallel_engine(data, actual_passes, scratch_u64);
    }
}

/// Performs a concurrent linear map upcasting elements from 32-bit workspace slices back into 64-bit records.
fn linear_upcast_u32_to_u64(src: &[u32], dst: &mut [u64]) {
    let n = src.len();
    let dst_ptr = dst.as_mut_ptr() as usize;
    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_idx, chunk)| {
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u64, n) };
        let start_pos = chunk_idx * CHUNK_SIZE;
        for (idx, &val) in chunk.iter().enumerate() {
            unsafe { *out.get_unchecked_mut(start_pos + idx) = val as u64; }
        }
    });
}
