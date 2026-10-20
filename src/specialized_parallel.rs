// src/specialized.rs

//! # Specialized Low-Bit Counting Sort Engines
//!
//! Highly optimized parallel counting sort routines for strictly bounded 8-bit and 16-bit datasets.

use rayon::prelude::*;

/// Specialized parallel counting sort for 8-bit values (range 0..255).
pub fn sapri_sort_8(data: &mut [u64]) {
    let n = data.len();
    if n < 2 { return; }

    if n < 5_000 {
        data.sort_unstable();
        return;
    }

    // 1. Parallel histogram accumulation (256 bins fit entirely in L1 cache)
    let counts = data.par_chunks(64 * 1024)
        .map(|chunk| {
            let mut hist = [0usize; 256];
            for &val in chunk {
                hist[(val & 0xFF) as usize] += 1;
            }
            hist
        })
        .reduce(
            || [0usize; 256],
            |mut acc, hist| {
                for i in 0..256 {
                    acc[i] += hist[i];
                }
                acc
            },
        );

    // 2. Linear high-speed reconstruction
    let mut idx = 0;
    for (val, &count) in counts.iter().enumerate() {
        if count > 0 {
            let end = idx + count;
            data[idx..end].fill(val as u64);
            idx = end;
        }
    }
}

/// Specialized parallel counting sort for 16-bit values (range 0..65535).
pub fn sapri_sort_16(data: &mut [u64]) {
    let n = data.len();
    if n < 2 { return; }

    if n < 10_000 {
        data.sort_unstable();
        return;
    }

    // 1. Parallel histogram accumulation (65,536 bins fit comfortably in L2/L3 cache)
    let counts = data.par_chunks(128 * 1024)
        .map(|chunk| {
            let mut hist = vec![0usize; 65536];
            for &val in chunk {
                hist[(val & 0xFFFF) as usize] += 1;
            }
            hist
        })
        .reduce(
            || vec![0usize; 65536],
            |mut acc, hist| {
                for i in 0..65536 {
                    acc[i] += hist[i];
                }
                acc
            },
        );

    // 2. Linear high-speed reconstruction
    let mut idx = 0;
    for (val, &count) in counts.iter().enumerate() {
        if count > 0 {
            let end = idx + count;
            data[idx..end].fill(val as u64);
            idx = end;
        }
    }
}
