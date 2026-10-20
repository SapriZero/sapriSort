// src/parallel.rs

//! # Parallel Hybrid Radix Sort Engine
//!
//! Multi-core parallel sorting routines powered by Rayon.
//! Optimized for high-throughput zero-allocation runs using chunked streaming,
//! thread-local concurrent histogram accumulation, L1 cache prefetching,
//! and adaptive monotonic fast-path evaluation.

use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
use crate::SapriParallelArena;

const CHUNK_SIZE: usize = 128 * 1024;

/// Element capacity threshold to trigger parallel execution.
pub const PARALLEL_THRESHOLD: usize = 2_000_000; 

/// Initializes the global multi-threaded Rayon worker pool.
pub fn init_parallel_engine(num_cores: usize) {
    let mut builder = ThreadPoolBuilder::new();
    if num_cores > 0 { builder = builder.num_threads(num_cores); }
    let _ = builder.build_global();
}

/// Main legacy/direct parallel core dispatcher.
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

/// Evaluates if a byte pass can be skipped based on histogram distribution uniformities.
#[inline(always)]
pub fn check_can_skip_pass(local_hists: &[[usize; 256]], total_elements: usize) -> bool {
    let mut totals = [0usize; 256];
    for hist in local_hists {
        for b in 0..256 {
            totals[b] += hist[b];
        }
    }
    for b in 0..256 {
        if totals[b] == total_elements {
            return true;
        }
    }
    false
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum DataLocation {
    OriginalData,
    ScratchA,
    ScratchB,
}

/// Holds thread-local execution boundary metrics to perform deterministic concurrent memory scattering.
pub struct ScatterMap {
    thread_offsets: Vec<[usize; 256]>,
}

// =========================================================================
// 32-BIT PARALLEL ENGINE
// =========================================================================

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

        match data_location {
            DataLocation::OriginalData => {
                let hists = phase_1_parallel_u64(data, shift);
                if check_can_skip_pass(&hists, n) {
                    if is_last {
                        data_location = DataLocation::OriginalData;
                    }
                    continue;
                }

                let smap = compute_scatter_map(&hists);
                if is_last {
                    scatter_parallel_u64_to_u64(data, &smap, scratch_u64, shift);
                    data.copy_from_slice(scratch_u64);
                    data_location = DataLocation::OriginalData;
                } else {
                    scatter_parallel_u64_to_u32(data, &smap, scratch_a, shift);
                    data_location = DataLocation::ScratchA;
                    src_ptr = scratch_a.as_mut_ptr();
                    dst_ptr = scratch_b.as_mut_ptr();
                }
            }
            _ => {
                let current_src = unsafe { std::slice::from_raw_parts(src_ptr, n) };
                let hists = phase_1_parallel_u32(current_src, shift);
                
                if check_can_skip_pass(&hists, n) {
                    if is_last {
                        if data_location == DataLocation::ScratchA {
                            linear_upcast_u32_to_u64(&scratch_a[..n], data);
                        } else {
                            linear_upcast_u32_to_u64(&scratch_b[..n], data);
                        }
                        data_location = DataLocation::OriginalData;
                    }
                    data_location = if data_location == DataLocation::ScratchA {
                        DataLocation::ScratchB
                    } else {
                        DataLocation::ScratchA
                    };
                    let temp = src_ptr;
                    src_ptr = dst_ptr;
                    dst_ptr = temp;
                    continue;
                }

                let smap = compute_scatter_map(&hists);
                if is_last {
                    scatter_parallel_u32_to_64(current_src, &smap, data, shift);
                    data_location = DataLocation::OriginalData;
                } else {
                    let current_dst = unsafe { std::slice::from_raw_parts_mut(dst_ptr, n) };
                    scatter_parallel_u32_to_u32(current_src, &smap, current_dst, shift);

                    data_location = if data_location == DataLocation::ScratchA {
                        DataLocation::ScratchB
                    } else {
                        DataLocation::ScratchA
                    };

                    let temp = src_ptr;
                    src_ptr = dst_ptr;
                    dst_ptr = temp;
                }
            }
        }
    }

    match data_location {
        DataLocation::OriginalData => {},
        DataLocation::ScratchA => {
            linear_upcast_u32_to_u64(&scratch_a[..n], data);
        },
        DataLocation::ScratchB => {
            linear_upcast_u32_to_u64(&scratch_b[..n], data);
        },
    }
}

// =========================================================================
// 64-BIT PARALLEL ENGINE
// =========================================================================

fn sapri_radix_64bit_parallel_engine(data: &mut [u64], passes: usize, scratch: &mut [u64]) {
    let n = data.len();
    if n == 0 { return; }

    let mut src_is_data = true;

    for i in 0..passes {
        let shift = (i * 8) as u32;

        if src_is_data {
            let hists = phase_1_parallel_u64(data, shift);
            if check_can_skip_pass(&hists, n) {
                continue;
            }
            let smap = compute_scatter_map(&hists);
            scatter_parallel_u64_to_u64(data, &smap, scratch, shift);
            src_is_data = false;
        } else {
            let hists = phase_1_parallel_u64(scratch, shift);
            if check_can_skip_pass(&hists, n) {
                continue;
            }
            let smap = compute_scatter_map(&hists);
            scatter_parallel_u64_to_u64(scratch, &smap, data, shift);
            src_is_data = true;
        }
    }

    if !src_is_data {
        data.copy_from_slice(scratch);
    }
}

// =========================================================================
// HISTOGRAMMING & SCATTER MAPS
// =========================================================================

#[inline(always)]
fn phase_1_parallel_u64(data: &[u64], shift: u32) -> Vec<[usize; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut hist = [0usize; 256];
            for &val in chunk { hist[((val >> shift) & 0xFF) as usize] += 1; }
            hist
        })
        .collect()
}

#[inline(always)]
fn phase_1_parallel_u32(data: &[u32], shift: u32) -> Vec<[usize; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut hist = [0usize; 256];
            for &val in chunk { hist[((val >> shift) & 0xFF) as usize] += 1; }
            hist
        })
        .collect()
}

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
// PREFETCH PARALLEL SCATTER LOOPS
// =========================================================================

fn scatter_parallel_u64_to_u32(src: &[u64], smap: &ScatterMap, dst: &mut [u32], shift: u32) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let dst_len = dst.len();
    let thread_offsets = &smap.thread_offsets;

    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(t, chunk)| {
        let mut ptrs = thread_offsets[t];
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u32, dst_len) };
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
                *out.get_unchecked_mut(pos) = val as u32;
                *ptrs.get_unchecked_mut(b) = pos + 1;
            }
        }
    });
}

fn scatter_parallel_u32_to_u32(src: &[u32], smap: &ScatterMap, dst: &mut [u32], shift: u32) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let dst_len = dst.len();
    let thread_offsets = &smap.thread_offsets;

    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(t, chunk)| {
        let mut ptrs = thread_offsets[t];
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u32, dst_len) };
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
                *out.get_unchecked_mut(pos) = val;
                *ptrs.get_unchecked_mut(b) = pos + 1;
            }
        }
    });
}

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

/// Dynamic Fast Sampling Arena Dispatcher with Monotonic Fast-Path & Real Max Calculation.
pub fn sapri_sort_parallel_in_arena(data: &mut [u64], passes: usize, arena: &mut SapriParallelArena) {
    let n = data.len();
    if n < 2 { return; }

    let gia_ordinato = data.par_chunks(CHUNK_SIZE).all(|chunk| {
        if chunk.len() < 2 { return true; }
        chunk.windows(2).all(|w| w[0] <= w[1])
    });
    if gia_ordinato {
        let mut globale_ok = true;
        for i in 1..(n / CHUNK_SIZE + 1) {
            let idx = i * CHUNK_SIZE;
            if idx < n && data[idx - 1] > data[idx] {
                globale_ok = false;
                break;
            }
        }
        if globale_ok { return; }
    }

    if n < PARALLEL_THRESHOLD {
        data.sort_unstable();
        return;
    }

    let max_val = data.par_iter().max().copied().unwrap_or(0);

    let bits = if max_val == 0 { 1 } else { 64 - max_val.leading_zeros() as usize };
    let actual_passes = ((bits + 7) / 8) as usize;
    let passes_to_use = if passes > 0 { passes } else { actual_passes };

    let (scratch_a, scratch_b, scratch_u64) = arena.get_buffers_mut(n);

    if max_val <= u32::MAX as u64 {
        sapri_radix_32bit_parallel_engine(data, passes_to_use.min(4), scratch_a, scratch_b, scratch_u64);
    } else {
        sapri_radix_64bit_parallel_engine(data, passes_to_use, scratch_u64);
    }
}

/// Safe linear upcasting from 32-bit slices to 64-bit target buffers.
fn linear_upcast_u32_to_u64(src: &[u32], dst: &mut [u64]) {
    src.par_chunks(CHUNK_SIZE)
        .zip(dst.par_chunks_mut(CHUNK_SIZE))
        .for_each(|(in_chunk, out_chunk)| {
            for (out_elem, &in_elem) in out_chunk.iter_mut().zip(in_chunk.iter()) {
                *out_elem = in_elem as u64;
            }
        });
}
