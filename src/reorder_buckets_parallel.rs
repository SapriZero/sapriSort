// src/reorder_buckets_parallel.rs

use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::_mm_prefetch;

const CHUNK_SIZE: usize = 128 * 1024;
const PARALLEL_THRESHOLD: usize = 2_000_000;

/// Inizializza il pool di Rayon con il numero di core desiderato.
/// Se passi `0`, Rayon userà il massimo dei core fisici/logici disponibili.
pub fn inizializza_motore_parallelo(num_cores: usize) {
    let mut builder = ThreadPoolBuilder::new();
    if num_cores > 0 {
        builder = builder.num_threads(num_cores);
    }
    // L'inizializzazione globale può fallire se già invocata, ignoriamo l'errore in sicurezza
    let _ = builder.build_global();
}

/// L'Orchestratore Parallelo Ibrido Adattivo 32/64-bit
pub fn sapri_radix_core(data: &mut [u64], max_val: u64) {
    let n = data.len();
    if n < 2 { return; }

    // FAST PATH ADATTIVO: Sotto la soglia usiamo il sorting seriale nativo
    if n < PARALLEL_THRESHOLD {
        data.sort_unstable();
        return;
    }

    let bits = if max_val == 0 { 1 } else { 64 - max_val.leading_zeros() as usize };
    let passes = (bits + 7) / 8;

    if max_val <= u32::MAX as u64 {
        sapri_radix_32bit_parallel_engine(data, passes);
    } else {
        sapri_radix_64bit_parallel_engine(data, passes);
    }
}

// =========================================================================
// MOTORE PARALLELO A 32-BIT
// =========================================================================
fn sapri_radix_32bit_parallel_engine(data: &mut [u64], passes: usize) {
    let n = data.len();
    let mut scratch_a = vec![0u32; n];
    let mut scratch_b = vec![0u32; n];

    for i in 0..passes {
        let shift = (i * 8) as u32;
        let is_last = i == passes - 1;

        if i == 0 {
            let hists = phase_1_parallel_u64(data, shift);
            let smap = compute_scatter_map(&hists);
            
            if is_last {
                let mut temp = vec![0u64; n];
                scatter_parallel_u64_to_u64(data, &smap, &mut temp, shift);
                data.copy_from_slice(&temp);
            } else {
                scatter_parallel_u64_to_u32(data, &smap, &mut scratch_a, shift);
            }
            continue;
        }

        if i % 2 != 0 {
            let hists = phase_1_parallel_u32(&scratch_a, shift);
            let smap = compute_scatter_map(&hists);
            if is_last {
                scatter_parallel_u32_to_64(&scratch_a, &smap, data, shift);
            } else {
                scatter_parallel_u32_to_u32(&scratch_a, &smap, &mut scratch_b, shift);
            }
        } else {
            let hists = phase_1_parallel_u32(&scratch_b, shift);
            let smap = compute_scatter_map(&hists);
            if is_last {
                scatter_parallel_u32_to_64(&scratch_b, &smap, data, shift);
            } else {
                scatter_parallel_u32_to_u32(&scratch_b, &smap, &mut scratch_a, shift);
            }
        }
    }
}

// =========================================================================
// MOTORE PARALLELO NATIVO A 64-BIT
// =========================================================================
fn sapri_radix_64bit_parallel_engine(data: &mut [u64], passes: usize) {
    let n = data.len();
    let mut scratch = vec![0u64; n];
    let mut src_is_data = true;

    for i in 0..passes {
        let shift = (i * 8) as u32;

        if src_is_data {
            let hists = phase_1_parallel_u64(data, shift);
            let smap = compute_scatter_map(&hists);
            scatter_parallel_u64_to_u64(data, &smap, &mut scratch, shift);
        } else {
            let hists = phase_1_parallel_u64(&scratch, shift);
            let smap = compute_scatter_map(&hists);
            scatter_parallel_u64_to_u64(&scratch, &smap, data, shift);
        }
        src_is_data = !src_is_data;
    }

    if !src_is_data {
        data.copy_from_slice(&scratch);
    }
}

// =========================================================================
// STRUTTURE E FUNZIONI DI DISTRIBUZIONE COALESCENTE (Rayon)
// =========================================================================

struct ScatterMap {
    thread_offsets: Vec<[usize; 256]>,
}

#[inline(always)]
fn phase_1_parallel_u64(data: &[u64], shift: u32) -> Vec<[usize; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut hist = [0usize; 256];
            for &val in chunk {
                hist[((val >> shift) & 0xFF) as usize] += 1;
            }
            hist
        })
        .collect()
}

#[inline(always)]
fn phase_1_parallel_u32(data: &[u32], shift: u32) -> Vec<[usize; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut hist = [0usize; 256];
            for &val in chunk {
                hist[((val >> shift) & 0xFF) as usize] += 1;
            }
            hist
        })
        .collect()
}

fn compute_scatter_map(local_hists: &[[usize; 256]]) -> ScatterMap {
    let num_threads = local_hists.len();
    let mut totals = [0usize; 256];
    
    for hist in local_hists {
        for b in 0..256 {
            totals[b] += hist[b];
        }
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
// OPERAZIONI DI SCATTER PARALLELE CON INIEZIONE DI HARDWARE PREFETCH
// =========================================================================

fn scatter_parallel_u64_to_u32(src: &[u64], smap: &ScatterMap, dst: &mut [u32], shift: u32) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let dst_len = dst.len();
    let thread_offsets = &smap.thread_offsets;

    src.par_chunks(CHUNK_SIZE)
        .enumerate()
        .for_each(|(t, chunk)| {
            let mut ptrs = thread_offsets[t];
            let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u32, dst_len) };
            let len = chunk.len();
            #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

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

    src.par_chunks(CHUNK_SIZE)
        .enumerate()
        .for_each(|(t, chunk)| {
            let mut ptrs = thread_offsets[t];
            let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u32, dst_len) };
            let len = chunk.len();
            #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

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

    src.par_chunks(CHUNK_SIZE)
        .enumerate()
        .for_each(|(t, chunk)| {
            let mut ptrs = thread_offsets[t];
            let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u64, dst_len) };
            let len = chunk.len();
            #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

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

    src.par_chunks(CHUNK_SIZE)
        .enumerate()
        .for_each(|(t, chunk)| {
            let mut ptrs = thread_offsets[t];
            let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u64, dst_len) };
            let len = chunk.len();
            #[cfg(target_arch = "x86_64")] const _MM_HINT_T0: i32 = 3;

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
