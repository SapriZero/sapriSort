// src/reorder_buckets2.rs

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};

/// ENGINE V2: Elaborazione a 16-bit per passata (65.536 Bucket)
/// Riduce il numero di passate (Max 2 per 32-bit, Max 4 per 64-bit)
pub fn sapri_radix_core(data: &mut [u64], max_val: u64) {
    let n = data.len();
    if n < 2 { return; }

    // Calcoliamo i bit effettivi necessari
    let bits = if max_val == 0 { 1 } else { 64 - max_val.leading_zeros() as usize };
    
    // Con 16 bit per passata, dividiamo per 16 invece che per 8
    let passes = (bits + 15) / 16;

    if max_val <= u32::MAX as u64 {
        sapri_radix_32bit_engine_v2(data, passes);
    } else {
        sapri_radix_64bit_engine_v2(data, passes);
    }
}

/// Ibrido 32-bit a 16 bit per passata (Fa al massimo 2 passate)
fn sapri_radix_32bit_engine_v2(data: &mut [u64], passes: usize) {
    let n = data.len();
    let mut scratch_a = vec![0u32; n];

    for i in 0..passes {
        let shift = (i * 16) as u32;
        let is_last = i == passes - 1;

        if i == 0 {
            // Prima passata: legge da u64 (data) e scrive in u32 (scratch_a) o torna in u64 se passes == 1
            let mut count = phase_1_u64_v2(data, shift);
            let mut total = 0usize;
            count.iter_mut().for_each(|c| { let old = *c; *c = total; total += old; });

            if is_last {
                let mut temp = vec![0u64; n];
                scatter_u64_to_u64_v2(data, &mut temp, &mut count, shift);
                data.copy_from_slice(&temp);
            } else {
                scatter_u64_to_u32_v2(data, &mut scratch_a, &mut count, shift);
            }
        } else {
            // Seconda e ultima passata: legge da u32 (scratch_a) e scrive direttamente in u64 (data)
            let mut count = phase_1_u32_v2(&scratch_a, shift);
            let mut total = 0usize;
            count.iter_mut().for_each(|c| { let old = *c; *c = total; total += old; });
            
            scatter_u32_to_64_v2(&scratch_a, data, &mut count, shift);
        }
    }
}

/// Nativo 64-bit a 16 bit per passata (Fa al massimo 4 passate)
fn sapri_radix_64bit_engine_v2(data: &mut [u64], passes: usize) {
    let n = data.len();

    if passes == 1 {
        let mut count = phase_1_u64_v2(data, 0);
        let mut scratch = vec![0u64; n];
        let mut total = 0usize;
        count.iter_mut().for_each(|c| { let old = *c; *c = total; total += old; });
        scatter_u64_to_u64_v2(data, &mut scratch, &mut count, 0);
        data.copy_from_slice(&scratch);
        return;
    }

    let mut scratch = vec![0u64; n];
    let mut src_is_data = true;

    for i in 0..passes {
        let shift = (i * 16) as u32;

        if src_is_data {
            let mut count = phase_1_u64_v2(data, shift);
            let mut total = 0usize;
            count.iter_mut().for_each(|c| { let old = *c; *c = total; total += old; });
            scatter_u64_to_u64_v2(data, &mut scratch, &mut count, shift);
        } else {
            let mut count = phase_1_u64_v2(&scratch, shift);
            let mut total = 0usize;
            count.iter_mut().for_each(|c| { let old = *c; *c = total; total += old; });
            scatter_u64_to_u64_v2(&scratch, data, &mut count, shift);
        }
        src_is_data = !src_is_data;
    }

    if !src_is_data {
        data.copy_from_slice(&scratch);
    }
}

// =========================================================================
// FUNZIONI ISTOGRAMMA A 16 BIT (65.536 combinazioni -> Maschera 0xFFFF)
// =========================================================================

#[inline(always)]
fn phase_1_u64_v2(data: &[u64], shift: u32) -> Vec<usize> {
    let mut count = vec![0usize; 65536]; // Allocazione dinamica controllata per la cache
    for &val in data {
        count[((val >> shift) & 0xFFFF) as usize] += 1;
    }
    count
}

#[inline(always)]
fn phase_1_u32_v2(data: &[u32], shift: u32) -> Vec<usize> {
    let mut count = vec![0usize; 65536];
    for &val in data {
        count[((val >> shift) & 0xFFFF) as usize] += 1;
    }
    count
}

// =========================================================================
// SCATTER REGULATION CON ADATTAMENTO DEI POINTER PER RANGE ESTESI (16-BIT)
// =========================================================================

#[inline(always)]
fn scatter_u64_to_u32_v2(src: &[u64], dst: &mut [u32], count: &mut [usize], shift: u32) {
    let dst_ptr = dst.as_mut_ptr();
    let len = src.len();
    #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

    for i in 0..len {
        // Lookahead ridotto a 4 elementi: la cache a 16-bit è molto più frammentata,
        // prefetch troppo distanti causerebbero stalli di pipeline caricando linee inutili.
        if i + 4 < len {
            unsafe {
                let lookahead_val = *src.get_unchecked(i + 4);
                let lookahead_b = ((lookahead_val >> shift) & 0xFFFF) as usize;
                let lookahead_pos = *count.get_unchecked(lookahead_b);
                #[cfg(target_arch = "x86_64")]
                _mm_prefetch(dst_ptr.add(lookahead_pos) as *const i8, _MM_HINT_T0);
            }
        }

        unsafe {
            let val = *src.get_unchecked(i);
            let b = ((val >> shift) & 0xFFFF) as usize;
            let pos = *count.get_unchecked(b);
            
            *dst_ptr.add(pos) = val as u32;
            *count.get_unchecked_mut(b) = pos + 1;
        }
    }
}

#[inline(always)]
fn scatter_u32_to_64_v2(src: &[u32], dst: &mut [u64], count: &mut [usize], shift: u32) {
    let dst_ptr = dst.as_mut_ptr();
    let len = src.len();
    #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

    for i in 0..len {
        if i + 4 < len {
            unsafe {
                let lookahead_val = *src.get_unchecked(i + 4);
                let lookahead_b = ((lookahead_val >> shift) & 0xFFFF) as usize;
                let lookahead_pos = *count.get_unchecked(lookahead_b);
                #[cfg(target_arch = "x86_64")]
                _mm_prefetch(dst_ptr.add(lookahead_pos) as *const i8, _MM_HINT_T0);
            }
        }

        unsafe {
            let val = *src.get_unchecked(i);
            let b = ((val >> shift) & 0xFFFF) as usize;
            let pos = *count.get_unchecked(b);
            
            *dst_ptr.add(pos) = val as u64;
            *count.get_unchecked_mut(b) = pos + 1;
        }
    }
}

#[inline(always)]
fn scatter_u64_to_u64_v2(src: &[u64], dst: &mut [u64], count: &mut [usize], shift: u32) {
    let dst_ptr = dst.as_mut_ptr();
    let len = src.len();
    #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

    for i in 0..len {
        if i + 4 < len {
            unsafe {
                let lookahead_val = *src.get_unchecked(i + 4);
                let lookahead_b = ((lookahead_val >> shift) & 0xFFFF) as usize;
                let lookahead_pos = *count.get_unchecked(lookahead_b);
                #[cfg(target_arch = "x86_64")]
                _mm_prefetch(dst_ptr.add(lookahead_pos) as *const i8, _MM_HINT_T0);
            }
        }

        unsafe {
            let val = *src.get_unchecked(i);
            let b = ((val >> shift) & 0xFFFF) as usize;
            let pos = *count.get_unchecked(b);
            
            *dst_ptr.add(pos) = val;
            *count.get_unchecked_mut(b) = pos + 1;
        }
    }
}
