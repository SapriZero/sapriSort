// src/reorder_buckets_parallel.rs

use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::_mm_prefetch;
use crate::SapriParallelArena;

const CHUNK_SIZE: usize = 128 * 1024;
pub const PARALLEL_THRESHOLD: usize = 2_000_000; 

pub fn inizializza_motore_parallelo(num_cores: usize) {
    let mut builder = ThreadPoolBuilder::new();
    if num_cores > 0 {
        builder = builder.num_threads(num_cores);
    }
    let _ = builder.build_global();
}

/// Orchestrator principale Ibrido modificato per accettare i buffer dell'arena esterna
pub fn sapri_radix_parallel_core(
    data: &mut [u64], 
    max_val: u64,
    scratch_u32_a: &mut [u32],
    scratch_u32_b: &mut [u32],
    scratch_u64: &mut [u64],
) {
    let n = data.len();
    if n < 2 { return; }

    // Se siamo sotto la soglia parallela, usiamo il sort di Rust in-place
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
// MOTORE PARALLELO A 32-BIT (ZERO-ALLOCATION)
// =========================================================================

#[derive(Clone, Debug)]
pub struct GlobalHistograms {
    // Matrice 4 passate x 256 bucket ciascuna
    pub counts: [[usize; 256]; 4],
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum DataLocation {
    OriginalData, // I dati validi sono in data (u64)
    ScratchA,     // I dati validi sono in scratch_a (u32)
    ScratchB,     // I dati validi sono in scratch_b (u32)
}

pub fn compute_global_histograms_u64(data: &[u64]) -> GlobalHistograms {
    use rayon::prelude::*;
    data.par_chunks(CHUNK_SIZE)
        .fold(
            || GlobalHistograms { counts: [[0; 256]; 4] },
            |mut local_hist, chunk| {
                for &val in chunk {
                    let b0 = (val & 0xFF) as usize;
                    let b1 = ((val >> 8) & 0xFF) as usize;
                    let b2 = ((val >> 16) & 0xFF) as usize;
                    let b3 = ((val >> 24) & 0xFF) as usize;

                    local_hist.counts[0][b0] += 1;
                    local_hist.counts[1][b1] += 1;
                    local_hist.counts[2][b2] += 1;
                    local_hist.counts[3][b3] += 1;
                }
                local_hist
            },
        )
        .reduce(
            || GlobalHistograms { counts: [[0; 256]; 4] },
            |mut total, local| {
                for pass in 0..4 {
                    for bucket in 0..256 {
                        total.counts[pass][bucket] += local.counts[pass][bucket];
                    }
                }
                total
            },
        )
}

#[inline(always)]
fn is_pass_skippable(global_hist: &GlobalHistograms, pass: usize, total_elements: usize) -> bool {
    for bucket in 0..256 {
        if global_hist.counts[pass][bucket] == total_elements {
            return true;
        }
    }
    false
}

// --- IL MOTORE DI COMPUTAZIONE CORRETTO ---

// In src/reorder_buckets_parallel.rs

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
                // 1. Calcoliamo l'istogramma locale (veloce e isolato, zero conflitti di cache pesanti)
                let hists = phase_1_parallel_u64(data, shift);
                
                // 2. Controlliamo subito se possiamo skippare lo scatter
                if check_can_skip_pass(&hists, n) {
                    if is_last {
                        // Se è l'ultima e non abbiamo mai mosso i dati, abbiamo finito.
                    }
                    // Resta in OriginalData, non fa scatter, va alla prossima passata
                    continue;
                }

                // 3. Se non è skippabile, procediamo con lo scatter normale
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
                
                // 1. Istogramma locale su u32
                let hists = phase_1_parallel_u32(current_src, shift);
                
                // 2. Controllo di skip al volo
                if check_can_skip_pass(&hists, n) {
                    if is_last {
                        // Se dobbiamo saltare l'ultima passata ma i dati sono dentro gli scratch,
                        // facciamo l'upcast lineare rapido monolitico verso data (u64)
                        if data_location == DataLocation::ScratchA {
                            linear_upcast_u32_to_u64(scratch_a, data);
                        } else {
                            linear_upcast_u32_to_u64(scratch_b, data);
                        }
                        data_location = DataLocation::OriginalData;
                    }
                    // Salto lo scatter, mantengo la posizione dei dati immutata, i puntatori non girano
                    continue;
                }

                // 3. Scatter reale
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
}

// =========================================================================
// MOTORE PARALLELO A 64-BIT (ZERO-ALLOCATION)
// =========================================================================
fn sapri_radix_64bit_parallel_engine(data: &mut [u64], passes: usize, scratch: &mut [u64]) {
    let n = data.len();
    let mut src_is_data = true;

    for i in 0..passes {
        let shift = (i * 8) as u32;

        if src_is_data {
            let hists = phase_1_parallel_u64(data, shift);
            if check_can_skip_pass(&hists, n) {
                scratch.copy_from_slice(data);
                src_is_data = !src_is_data;
                continue;
            }
            
            let smap = compute_scatter_map(&hists);
            scatter_parallel_u64_to_u64(data, &smap, scratch, shift);
        } else {
            let hists = phase_1_parallel_u64(scratch, shift);
            if check_can_skip_pass(&hists, n) {
                data.copy_from_slice(scratch);
                src_is_data = !src_is_data;
                continue;
            }
            
            let smap = compute_scatter_map(&hists);
            scatter_parallel_u64_to_u64(scratch, &smap, data, shift);
        }
        src_is_data = !src_is_data;
    }

    if !src_is_data {
        data.copy_from_slice(scratch);
    }
}

fn scatter_flat_u32_to_64(src: &[u32], dst: &mut [u64]) {
    let dst_ptr = dst.as_mut_ptr() as usize;
    let n = src.len();
    dst.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_idx, chunk)| {
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u64, n) };
        let start_pos = chunk_idx * CHUNK_SIZE;
        for (idx, &val) in chunk.iter().enumerate() {
            unsafe { *out.get_unchecked_mut(start_pos + idx) = val as u64; }
        }
    });
}

// =========================================================================
// FUNZIONI DI FASE 1 PARALLELA
// =========================================================================
pub struct ScatterMap {
    thread_offsets: Vec<[usize; 256]>,
}

#[inline(always)]
fn phase_1_parallel_u64(data: &[u64], shift: u32) -> Vec<[usize; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut _hist = [0usize; 256];
            for &val in chunk {
                _hist[((val >> shift) & 0xFF) as usize] += 1;
            }
            _hist
        })
        .collect()
}

#[inline(always)]
fn phase_1_parallel_u32(data: &[u32], shift: u32) -> Vec<[usize; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut _hist = [0usize; 256];
            for &val in chunk {
                _hist[((val >> shift) & 0xFF) as usize] += 1;
            }
            _hist
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


pub fn sapri_sort_parallel_in_arena(data: &mut [u64], passes: usize, arena: &mut SapriParallelArena) {
    let n = data.len();
    if n < 2 { return; }

    // 1. ⚡ CAMPIONAMENTO LINEARE (Zero impatto sulla cache di Rayon)
    // Prendiamo i primi 8 elementi: sono tutti nella stessa Cache Line!
    let mut sample_min = u64::MAX;
    let mut sample_max = 0;
    
    let sample_limit = n.min(8);
    for i in 0..sample_limit {
        let v = data[i];
        if v < sample_min { sample_min = v; }
        if v > sample_max { sample_max = v; }
    }
    
    let estimated_range = sample_max.saturating_sub(sample_min);

    // 2. 🧠 CALCOLO SOGLIA DINAMICA (Invariato)
    let required_bits = 64 - estimated_range.leading_zeros();
    let dynamic_threshold = match required_bits {
        0..=8   => 800_000,   
        9..=16  => 1_800_000, // Il tuo sweet spot!
        17..=24 => 3_000_000, 
        _       => 4_500_000, 
    };

    if n < dynamic_threshold {
        data.sort_unstable();
        return;
    }

    // 3. Protezione extra
    if estimated_range > 0 && (n as u64) / estimated_range > 500 {
        data.sort_unstable();
        return;
    }

    // 4. Esecuzione del motore reale
    let (scratch_a, scratch_b, scratch_u64) = arena.get_buffers_mut(n);
    sapri_radix_32bit_parallel_engine(data, passes, scratch_a, scratch_b, scratch_u64);
}

/// Copia lineare e downcast ultra-rapido da u64 a u32 (senza ordinamento)
fn linear_downcast_u64_to_u32(src: &[u64], dst: &mut [u32]) {
    let n = src.len();
    let dst_ptr = dst.as_mut_ptr() as usize;
    
    src.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_idx, chunk)| {
        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u32, n) };
        let start_pos = chunk_idx * CHUNK_SIZE;
        for (idx, &val) in chunk.iter().enumerate() {
            unsafe { *out.get_unchecked_mut(start_pos + idx) = val as u32; }
        }
    });
}

/// Copia lineare e upcast ultra-rapido da u32 a u64 (senza ordinamento)
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
