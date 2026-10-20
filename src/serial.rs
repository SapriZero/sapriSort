// src/serial.rs

//! # Sequential Hybrid Radix Sort Core
//!
//! Provides fundamental single-core sequential sorting routines and base fallback operations.

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::_mm_prefetch;

/// The Grand Orchestrator: dispatches execution to specialized 32-bit or 64-bit serial engines.
pub fn sapri_radix_core(data: &mut [u64], max_val: u64) {
    let n = data.len();
    if n < 2 { return; }

    let bits = if max_val == 0 { 1 } else { 64 - max_val.leading_zeros() as usize };
    let passes = (bits + 7) / 8;

    if max_val <= u32::MAX as u64 {
        sapri_radix_32bit_engine(data, passes);
    } else {
        sapri_radix_64bit_engine(data, passes);
    }
}

/// Specialized 32-bit Hybrid Engine (Single-Core).
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

#[inline(always)]
fn phase_1_u64(data: &[u64], shift: u32) -> [usize; 256] {
    let mut count = [0usize; 256];
    for &val in data {
        count[((val >> shift) & 0xFF) as usize] += 1;
    }
    count
}

#[inline(always)]
fn phase_1_u32(data: &[u32], shift: u32) -> [usize; 256] {
    let mut count = [0usize; 256];
    for &val in data {
        count[((val >> shift) & 0xFF) as usize] += 1;
    }
    count
}

#[inline(always)]
fn scatter_u64_to_u32(src: &[u64], dst: &mut [u32], count: &mut [usize; 256], shift: u32) {
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

#[inline(always)]
fn scatter_u32_to_u32(src: &[u32], dst: &mut [u32], count: &mut [usize; 256], shift: u32) {
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

#[inline(always)]
fn scatter_u32_to_64(src: &[u32], dst: &mut [u64], count: &mut [usize; 256], shift: u32) {
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

#[inline(always)]
fn scatter_u64_to_u64(src: &[u64], dst: &mut [u64], count: &mut [usize; 256], shift: u32) {
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
