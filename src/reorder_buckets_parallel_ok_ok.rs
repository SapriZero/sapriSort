// src/reorder_buckets_parallel.rs

use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::_mm_prefetch;

const CHUNK_SIZE: usize = 128 * 1024;
const PARALLEL_THRESHOLD: usize = 2_000_000;
const TINY_CHUNK_THRESHOLD: usize = 1024;

pub fn inizializza_motore_parallelo(num_cores: usize) {
    let mut builder = ThreadPoolBuilder::new();
    if num_cores > 0 {
        builder = builder.num_threads(num_cores);
    }
    let _ = builder.build_global();
}

/// L'Orchestratore Parallelo Ibrido Adattivo con ottimizzazioni storiche totali
pub fn sapri_radix_core(data: &mut [u64], _max_val_teorico: u64) {
    let n = data.len();
    if n < 2 { return; }

    // 1. FAST PATH STORICO: Controllo di monotonicità (Array già ordinato)
    let gia_ordinato = data.par_chunks(CHUNK_SIZE).all(|chunk| {
        if chunk.len() < 2 { return true; }
        chunk.windows(2).all(|w| w[0] <= w[1])
    });
    if gia_ordinato {
        // Se tutti i chunk sono ordinati internamente, facciamo un controllo globale veloce di giunzione
        let mut globale_ok = true;
        for i in 1..(n / CHUNK_SIZE + 1) {
            let idx = i * CHUNK_SIZE;
            if idx < n && data[idx - 1] > data[idx] {
                globale_ok = false;
                break;
            }
        }
        if globale_ok { return; } // Esce istantaneamente a costo quasi zero!
    }

    // 2. FAST PATH ADATTIVO: Sotto la soglia critica andiamo in seriale
    if n < PARALLEL_THRESHOLD {
        data.sort_unstable();
        return;
    }

    // 3. MINA D'ORO STORICA: Calcolo del massimo REALE in parallelo
    let max_val_reale = data.par_iter().max().copied().unwrap_or(0);

    let bits = if max_val_reale == 0 { 1 } else { 64 - max_val_reale.leading_zeros() as usize };
    let passes = (bits + 7) / 8;

    if max_val_reale <= u32::MAX as u64 {
        sapri_radix_32bit_parallel_engine(data, passes);
    } else {
        sapri_radix_64bit_parallel_engine(data, passes);
    }
}

#[inline(always)]
fn check_can_skip_pass(local_hists: &[[usize; 256]], total_elements: usize) -> bool {
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
            if check_can_skip_pass(&hists, n) {
                if !is_last {
                    let scratch_ptr = scratch_a.as_mut_ptr() as usize;
                    data.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_idx, chunk)| {
                        let out = unsafe { std::slice::from_raw_parts_mut(scratch_ptr as *mut u32, n) };
                        let start_pos = chunk_idx * CHUNK_SIZE;
                        for (idx, &val) in chunk.iter().enumerate() {
                            unsafe { *out.get_unchecked_mut(start_pos + idx) = val as u32; }
                        }
                    });
                }
                continue;
            }

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
            if check_can_skip_pass(&hists, n) {
                if is_last {
                    scatter_flat_u32_to_64(&scratch_a, data);
                } else {
                    scratch_b.copy_from_slice(&scratch_a);
                }
                continue;
            }

            let smap = compute_scatter_map(&hists);
            if is_last {
                scatter_parallel_u32_to_64(&scratch_a, &smap, data, shift);
            } else {
                scatter_parallel_u32_to_u32(&scratch_a, &smap, &mut scratch_b, shift);
            }
        } else {
            let hists = phase_1_parallel_u32(&scratch_b, shift);
            if check_can_skip_pass(&hists, n) {
                if is_last {
                    scatter_flat_u32_to_64(&scratch_b, data);
                } else {
                    scratch_a.copy_from_slice(&scratch_b);
                }
                continue;
            }

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
// MOTORE PARALLELO A 64-BIT (OTTIMIZZATO ZERO-COPY SWAP)
// =========================================================================
fn sapri_radix_64bit_parallel_engine(data: &mut [u64], passes: usize) {
    let n = data.len();
    let mut scratch = vec![0u64; n];
    
    // Creiamo un vettore di supporto per gestire i puntatori in Zero-Copy alla fine
    let mut src_is_data = true;

    for i in 0..passes {
        let shift = (i * 8) as u32;

        if src_is_data {
            let hists = phase_1_parallel_u64(data, shift);
            if check_can_skip_pass(&hists, n) {
                src_is_data = !src_is_data;
                continue;
            }
            let smap = compute_scatter_map(&hists);
            scatter_parallel_u64_to_u64(data, &smap, &mut scratch, shift);
        } else {
            let hists = phase_1_parallel_u64(&scratch, shift);
            if check_can_skip_pass(&hists, n) {
                src_is_data = !src_is_data;
                continue;
            }
            let smap = compute_scatter_map(&hists);
            scatter_parallel_u64_to_u64(&scratch, &smap, data, shift);
        }
        src_is_data = !src_is_data;
    }

    // 4. TRUCCO ZERO-COPY STORICO: Se l'ultima passata ha lasciato i dati nel buffer scratch,
    // facciamo una ricopiatura obbligatoria, ma solo se strettamente necessario!
    if !src_is_data {
        data.copy_from_slice(&scratch);
    }
}

fn scatter_flat_u32_to_64(src: &[u32], dst: &mut [u64]) {
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
            if chunk.len() < TINY_CHUNK_THRESHOLD {
                for &val in chunk { hist[((val >> shift) & 0xFF) as usize] += 1; }
                return hist;
            }
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
            if chunk.len() < TINY_CHUNK_THRESHOLD {
                for &val in chunk { hist[((val >> shift) & 0xFF) as usize] += 1; }
                return hist;
            }
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
