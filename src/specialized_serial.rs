// src/specialized_serial.rs

//! # Specialized Serial Low-Bit Counting Sort Engines
//!
//! Highly optimized single-core counting sort routines for strictly bounded 8-bit and 16-bit datasets
//! with zero-allocation threshold guards.

/// Specialized serial counting sort for 8-bit values (range 0..255).
#[inline(always)]
pub fn sapri_sort_8_serial(data: &mut [u64]) {
    let n = data.len();
    if n < 2 { return; }

    // Guardia di soglia: sotto i 2000 elementi il quicksort nativo è imbattibile e zero-overhead
    if n < 2000 {
        data.sort_unstable();
        return;
    }

    let mut hist = [0usize; 256];
    for val in data.iter() {
        hist[((*val) & 0xFF) as usize] += 1;
    }

    let mut idx = 0;
    for (val, &count) in hist.iter().enumerate() {
        if count > 0 {
            let end = idx + count;
            data[idx..end].fill(val as u64);
            idx = end;
        }
    }
}

/// Specialized serial counting sort for 16-bit values (range 0..65535) utilizing an external scratchpad.
#[inline(always)]
pub fn sapri_sort_16_serial_with_scratch(data: &mut [u64], scratch: &mut [usize]) {
    let n = data.len();
    if n < 2 { return; }

    // Guardia di soglia: sotto i 5000 elementi evitiamo l'istogramma da 65k
    if n < 5000 {
        data.sort_unstable();
        return;
    }

    assert!(scratch.len() >= 65536, "Scratch buffer for 16-bit sort must be at least 65536 elements");
    let hist = &mut scratch[..65536];
    
    hist.fill(0);

    for val in data.iter() {
        hist[((*val) & 0xFFFF) as usize] += 1;
    }

    let mut idx = 0;
    for (val, &count) in hist.iter().enumerate() {
        if count > 0 {
            let end = idx + count;
            data[idx..end].fill(val as u64);
            idx = end;
        }
    }
}

/// Convenience wrapper preserving backward compatibility with automatic threshold routing.
pub fn sapri_sort_16_serial(data: &mut [u64]) {
    let n = data.len();
    if n < 5000 {
        data.sort_unstable();
        return;
    }
    let mut scratch = vec![0usize; 65536];
    sapri_sort_16_serial_with_scratch(data, &mut scratch);
}
