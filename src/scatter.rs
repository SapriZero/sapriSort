// src/scatter.rs

//! # Low-Level Scatter Kernels & Memory Utilities
//!
//! Provides high-performance memory transformation routines, linear upcasting,
//! and hardware prefetching helpers optimized for cache-line efficiency.

use rayon::prelude::*;

const CHUNK_SIZE: usize = 128 * 1024;

/// Safe linear upcasting from 32-bit source slices to 64-bit target buffers using Rayon parallel chunks.
#[inline(always)]
pub fn linear_upcast_u32_to_u64(src: &[u32], dst: &mut [u64]) {
    src.par_chunks(CHUNK_SIZE)
        .zip(dst.par_chunks_mut(CHUNK_SIZE))
        .for_each(|(in_chunk, out_chunk)| {
            for (out_elem, &in_elem) in out_chunk.iter_mut().zip(in_chunk.iter()) {
                *out_elem = in_elem as u64;
            }
        });
}

/// Flat sequential scattering from 32-bit source buffers to 64-bit destination arrays.
#[inline(always)]
pub fn scatter_flat_u32_to_64(src: &[u32], dst: &mut [u64]) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let n = src.len();
    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_idx, chunk)| {
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u64, n) };
        let start_pos = chunk_idx * CHUNK_SIZE;
        for (idx, &val) in chunk.iter().enumerate() {
            unsafe { *out.get_unchecked_mut(start_pos + idx) = val as u64; }
        }
    });
}
