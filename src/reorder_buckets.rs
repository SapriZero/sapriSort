//! # Sequential Hybrid Radix Sort Core
//!
//! Provides the fundamental single-core sequential sort routines and atomic sub-operations.
//! It includes low-overhead 32-bit downcast engines and native 64-bit routines, paired 
//! with hardware-level prefetching hints for L1/L2 cache performance optimizations.

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_prefetch};

/// The Grand Orchestrator: dispatches execution to the specialized 32-bit or 64-bit engine.
///
/// It checks the `max_val` to calculate the required number of radix bits and passes, 
/// minimizing memory footprints when values fit into a standard 32-bit integer.
pub fn sapri_radix_core(data: &mut [u64], max_val: u64) {
    let n = data.len();
    if n < 2 { return; }

    println!("--- DIAGNOSTIC: n = {}, max_val = {} ---", n, max_val);

    let bits = if max_val == 0 { 1 } else { 64 - max_val.leading_zeros() as usize };
    let passes = (bits + 7) / 8;

    if max_val <= u32::MAX as u64 {
        sapri_radix_32bit_engine(data, passes);
    } else {
        sapri_radix_64bit_engine(data, passes);
    }
}

/// Specialized 32-bit Hybrid Engine (Single-Core).
///
/// Uses standard temporary heap vectors to transition from 64-bit values to 32-bit intermediate
/// structures, accelerating subsequent distribution passes through cache-friendly lookups.
fn sapri_radix_32bit_engine(data: &mut [u64], passes: usize) {
    let n = data.len();
    let mut scratch_a = vec![0u32; n];
    let mut scratch_b = vec![0u32; n];

    for i in 0..passes {
        let shift = (i * 8) as u32;
        let is_last = i == passes - 1;

        if i == 0 {
            process_first_pass_32bit(data, &mut scratch_a, shift, is_last);
            continue;
        }

        if is_last {
            process_last_pass_32bit(data, &scratch_a, &scratch_b, shift, i);
            continue;
        }

        process_intermediate_pass_32bit(&mut scratch_a, &mut scratch_b, shift, i);
    }
}

/// Native 64-bit Engine (Single-Core).
///
/// Executed as a fallback logic when items inside the dataset exceed the maximum value 
/// boundaries of a 32-bit unsigned integer space.
fn sapri_radix_64bit_engine(data: &mut [u64], passes: usize) {
    let n = data.len();

    if passes == 1 {
        let mut count = phase_1_u64(data, 0);
        let mut total = 0usize;
        count.iter_mut().for_each(|c| {
            let old = *c;
            *c = total;
            total += old;
        });

        let mut scratch = vec![0u64; n];
        scatter_u64_to_u64(data, &mut scratch, &mut count, 0);
        data.copy_from_slice(&scratch);
        return;
    }

    let mut scratch = vec![0u64; n];
    let mut src_is_data = true;

    for i in 0..passes {
        let shift = (i * 8) as u32;

        if src_is_data {
            let mut count = phase_1_u64(data, shift);
            let mut total = 0usize;
            count.iter_mut().for_each(|c| {
                let old = *c;
                *c = total;
                total += old;
            });
            scatter_u64_to_u64(data, &mut scratch, &mut count, shift);
        } else {
            let mut count = phase_1_u64(&scratch, shift);
            let mut total = 0usize;
            count.iter_mut().for_each(|c| {
                let old = *c;
                *c = total;
                total += old;
            });
            scatter_u64_to_u64(&scratch, data, &mut count, shift);
        }
        src_is_data = !src_is_data;
    }

    if !src_is_data {
        data.copy_from_slice(&scratch);
    }
}

// =========================================================================
// INLINED STEPS FOR THE SEQUENTIAL 32-BIT ENGINE
// =========================================================================

/// Processes the foundational distribution pass, downcasting elements to 32-bit slots.
#[inline(always)]
fn process_first_pass_32bit(data: &mut [u64], scratch_a: &mut [u32], shift: u32, is_last: bool) {
    let mut count = phase_1_u64(data, shift);
    let mut total = 0usize;
    count.iter_mut().for_each(|c| {
        let old = *c;
        *c = total;
        total += old;
    });

    if is_last {
        let mut temp = vec![0u64; data.len()];
        scatter_u64_to_u64(data, &mut temp, &mut count, shift);
        data.copy_from_slice(&temp);
    } else {
        scatter_u64_to_u32(data, scratch_a, &mut count, shift);
    }
}

/// Loops through intermediary bit shifts using exclusively 32-bit workspace fields.
#[inline(always)]
fn process_intermediate_pass_32bit(scratch_a: &mut [u32], scratch_b: &mut [u32], shift: u32, i: usize) {
    if i % 2 != 0 {
        let mut count = phase_1_u32(scratch_a, shift);
        let mut total = 0usize;
        count.iter_mut().for_each(|c| {
            let old = *c;
            *c = total;
            total += old;
        });
        scatter_u32_to_u32(scratch_a, scratch_b, &mut count, shift);
    } else {
        let mut count = phase_1_u32(scratch_b, shift);
        let mut total = 0usize;
        count.iter_mut().for_each(|c| {
            let old = *c;
            *c = total;
            total += old;
        });
        scatter_u32_to_u32(scratch_b, scratch_a, &mut count, shift);
    }
}

/// Finalizes the engine logic by upcasting data structures back onto the master 64-bit data stream.
#[inline(always)]
fn process_last_pass_32bit(data: &mut [u64], scratch_a: &[u32], scratch_b: &[u32], shift: u32, i: usize) {
    if i % 2 != 0 {
        let mut count = phase_1_u32(scratch_a, shift);
        let mut total = 0usize;
        count.iter_mut().for_each(|c| {
            let old = *c;
            *c = total;
            total += old;
        });
        scatter_u32_to_64(scratch_a, data, &mut count, shift);
    } else {
        let mut count = phase_1_u32(scratch_b, shift);
        let mut total = 0usize;
        count.iter_mut().for_each(|c| {
            let old = *c;
            *c = total;
            total += old;
        });
        scatter_u32_to_64(scratch_b, data, &mut count, shift);
    }
}

// =========================================================================
// ATOMIC & CONDITIONAL SHARED UTILITIES (EXPORTED TO PARALLEL)
// =========================================================================

/// Evaluates histograms to decide whether an entire sorting pass can be completely skipped.
#[inline(always)]
pub(crate) fn check_can_skip_pass(local_hists: &[[usize; 256]], total_elements: usize) -> bool {
    let mut totals = [0usize; 256];
    for hist in local_hists {
        for b in 0..256 {
            totals[b] += hist[b];
        }
    }
    for b in 0..256 {
        if totals[b] == total_elements { return true; }
    }
    false
}

/// Accumulates byte frequency distribution histograms from a 64-bit slice.
#[inline(always)]
pub(crate) fn phase_1_u64(data: &[u64], shift: u32) -> [usize; 256] {
    let mut count = [0usize; 256];
    for &val in data {
        count[((val >> shift) & 0xFF) as usize] += 1;
    }
    count
}

/// Accumulates byte frequency distribution histograms from a 32-bit slice.
#[inline(always)]
pub(crate) fn phase_1_u32(data: &[u32], shift: u32) -> [usize; 256] {
    let mut count = [0usize; 256];
    for &val in data {
        count[((val >> shift) & 0xFF) as usize] += 1;
    }
    count
}

// =========================================================================
// HIGH-PERFORMANCE SCATTER OPERATIONS (EXPORTED TO PARALLEL)
// =========================================================================

/// Scatters values from a 64-bit slice down into a 32-bit workspace with software prefetching.
#[inline(always)]
pub(crate) fn scatter_u64_to_u32(src: &[u64], dst: &mut [u32], count: &mut [usize; 256], shift: u32) {
    let dst_ptr = dst.as_mut_ptr();
    let len = src.len();
    #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

    for i in 0..len {
        if i + 8 < len {
            unsafe {
                let lookahead_val = *src.get_unchecked(i + 8);
                let lookahead_b = ((lookahead_val >> shift) & 0xFF) as usize;
                let lookahead_pos = *count.get_unchecked(lookahead_b);
                #[cfg(target_arch = "x86_64")]
                _mm_prefetch(dst_ptr.add(lookahead_pos) as *const i8, _MM_HINT_T0);
            }
        }
        unsafe {
            let val = *src.get_unchecked(i);
            let b = ((val >> shift) & 0xFF) as usize;
            let pos = *count.get_unchecked(b);
            *dst_ptr.add(pos) = val as u32;
            *count.get_unchecked_mut(b) = pos + 1;
        }
    }
}

/// Scatters values between two distinct 32-bit workspaces with software prefetching.
#[inline(always)]
pub(crate) fn scatter_u32_to_u32(src: &[u32], dst: &mut [u32], count: &mut [usize; 256], shift: u32) {
    let dst_ptr = dst.as_mut_ptr();
    let len = src.len();
    #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

    for i in 0..len {
        if i + 16 < len {
            unsafe {
                let lookahead_val = *src.get_unchecked(i + 16);
                let lookahead_b = ((lookahead_val >> shift) & 0xFF) as usize;
                let lookahead_pos = *count.get_unchecked(lookahead_b);
                #[cfg(target_arch = "x86_64")]
                _mm_prefetch(dst_ptr.add(lookahead_pos) as *const i8, _MM_HINT_T0);
            }
        }
        unsafe {
            let val = *src.get_unchecked(i);
            let b = ((val >> shift) & 0xFF) as usize;
            let pos = *count.get_unchecked(b);
            *dst_ptr.add(pos) = val;
            *count.get_unchecked_mut(b) = pos + 1;
        }
    }
}

/// Scatters values from a 32-bit workspace up into a 64-bit slice with software prefetching.
#[inline(always)]
pub(crate) fn scatter_u32_to_64(src: &[u32], dst: &mut [u64], count: &mut [usize; 256], shift: u32) {
    let dst_ptr = dst.as_mut_ptr();
    let len = src.len();
    #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

    for i in 0..len {
        if i + 16 < len {
            unsafe {
                let lookahead_val = *src.get_unchecked(i + 16);
                let lookahead_b = ((lookahead_val >> shift) & 0xFF) as usize;
                let lookahead_pos = *count.get_unchecked(lookahead_b);
                #[cfg(target_arch = "x86_64")]
                _mm_prefetch(dst_ptr.add(lookahead_pos) as *const i8, _MM_HINT_T0);
            }
        }
        unsafe {
            let val = *src.get_unchecked(i);
            let b = ((val >> shift) & 0xFF) as usize;
            let pos = *count.get_unchecked(b);
            *dst_ptr.add(pos) = val as u64;
            *count.get_unchecked_mut(b) = pos + 1;
        }
    }
}

/// Scatters values between two distinct 64-bit slices with software prefetching.
#[inline(always)]
pub(crate) fn scatter_u64_to_u64(src: &[u64], dst: &mut [u64], count: &mut [usize; 256], shift: u32) {
    let dst_ptr = dst.as_mut_ptr();
    let len = src.len();
    #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

    for i in 0..len {
        if i + 8 < len {
            unsafe {
                let lookahead_val = *src.get_unchecked(i + 8);
                let lookahead_b = ((lookahead_val >> shift) & 0xFF) as usize;
                let lookahead_pos = *count.get_unchecked(lookahead_b);
                #[cfg(target_arch = "x86_64")]
                _mm_prefetch(dst_ptr.add(lookahead_pos) as *const i8, _MM_HINT_T0);
            }
        }
        unsafe {
            let val = *src.get_unchecked(i);
            let b = ((val >> shift) & 0xFF) as usize;
            let pos = *count.get_unchecked(b);
            *dst_ptr.add(pos) = val;
            *count.get_unchecked_mut(b) = pos + 1;
        }
    }
}
