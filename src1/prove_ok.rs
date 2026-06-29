// src/prove.rs
use crate::sort_helper::*; // Importiamo tutto dal nuovo helper
use rayon::prelude::*;

pub const PROGRESS_STEP:   usize = 1 << 17;
pub const PROGRESS_MASK:   usize = PROGRESS_STEP - 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortStrategy { Auto, CountingU8, RadixSerial, RadixParallel, RadixAdaptive }

pub struct SortOptions {
    pub strategy: SortStrategy,
    pub known_max: Option<u32>,
    pub force_passes: Option<usize>,
}

impl Default for SortOptions {
    fn default() -> Self {
        Self { strategy: SortStrategy::Auto, known_max: None, force_passes: None }
    }
}

// ============================================================
// API PUBBLICA
// ============================================================

pub fn sapri_sort(data: &mut [u32], depth: usize) {
    execute_sapri_sort(data, depth, true, 0, data.len());
}

pub fn execute_sapri_sort(data: &mut [u32], depth: usize, use_adaptive: bool, start: usize, end: usize) {
    let len = data.len();
    let real_end = if end == 0 || end > len { len } else { end };
    let n = real_end - start;
    
    // Fast Path Aggressivo
    if n < TINY_UNSTABLE {
        data[start..real_end].sort_unstable();
        return;
    }
    
    // Fast Path per casi a 1 passata (L0)
    if depth == 1 {
        counting_sort_u8(&mut data[start..real_end]);
        return;
    }
    // 2. Fast Path Specializzati (Solo sulla chiamata iniziale!)
    if start == 0 && real_end == len {
        if depth == 1 {
            counting_sort_u8(data);
            return;
        }
        if depth == 2 && n < 50_000_000 {
            counting_sort_u16(data);
            return;
        }
    }

    let mut scratch = vec![0u32; len];
    inner_execute_with_scratch(data, &mut scratch, depth, use_adaptive, start, real_end);
}

pub fn inner_execute_with_scratch(
    data: &mut [u32], 
    scratch: &mut [u32], 
    depth: usize, 
    use_adaptive: bool, 
    start: usize, 
    end: usize
) {
    let n = end - start;
    let effective_depth = map_depth(depth);
    
    // 1. SOGLIA CRITICA: Array Massicci (Cache Trashing)
    // Se l'array è enorme (> 5M), l'MSD ricorsivo rischia di frammentare troppo la cache.
    // L'LSD (RadixSerial) è sequenziale e molto più amico dei prefetcher della CPU.
    if n > 5_000_000 {
        radix_serial_logic(&mut data[start..end], scratch, effective_depth);
        return;
    }

    // 2. SELEZIONE STRATEGICA (Adattivo vs Seriale)
    // Usiamo MSD (Adattivo) solo se:
    // - L'utente ha chiesto l'adattivo (use_adaptive)
    // - La profondità è significativa (> 2)
    // - Non siamo nel caso "massiccio" gestito sopra
    if use_adaptive && effective_depth > 2 {
        msd_recursive(
            &mut data[start..end], 
            scratch, 
            0, 
            n, 
            ((effective_depth - 1) * 8) as i32
        );
    } else {
        // Fallback al robusto Radix Seriale (LSD)
        radix_serial_logic(&mut data[start..end], scratch, effective_depth);
    }
}

// ============================================================
// LOGICA PARALLELA
// ============================================================
pub fn phase_1_parallel(data: &[u32], shift: u32) -> Vec<[u32; 256]> {
    data.par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let mut hist = [0u32; 256];
            for &val in chunk { hist[((val >> shift) & 0xFF) as usize] += 1; }
            hist
        })
        .collect()
}

#[inline]
pub fn phase_1_serial(data: &[u32], shift: u32) -> [usize; 256] {
    let mut count = [0usize; 256];
    for &val in data { count[((val >> shift) & 0xFF) as usize] += 1; }
    count
}

pub struct ScatterMap { pub thread_offsets: Vec<[u32; 256]>, pub global_offsets: [u32; 256] }

pub fn compute_scatter_map(local_hists: &[[u32; 256]]) -> ScatterMap {
    let num_threads = local_hists.len();
    let mut totals = [0u32; 256];
    for hist in local_hists { for b in 0..256 { totals[b] += hist[b]; } }
    let mut global_offsets = [0u32; 256];
    let mut running = 0u32;
    for b in 0..256 { global_offsets[b] = running; running += totals[b]; }
    let mut thread_offsets = vec![[0u32; 256]; num_threads];
    for b in 0..256 {
        let mut cursor = global_offsets[b];
        for t in 0..num_threads {
            thread_offsets[t][b] = cursor;
            cursor += local_hists[t][b];
        }
    }
    ScatterMap { thread_offsets, global_offsets }
}

fn radix_parallel_n_passes(data: &mut Vec<u32>, passes: usize) {
    let size = data.len();
    let mut buf_b = vec![0u32; size];
    let mut buf_a = std::mem::take(data);
    let mut src_is_a = true;

    for i in 0..passes {
        let shift = (i * 8) as u32;
        if src_is_a {
            let hists = phase_1_parallel(&buf_a, shift);
            let smap = compute_scatter_map(&hists);
            scatter_parallel(&buf_a, &smap, &mut buf_b, shift);
        } else {
            let hists = phase_1_parallel(&buf_b, shift);
            let smap = compute_scatter_map(&hists);
            scatter_parallel(&buf_b, &smap, &mut buf_a, shift);
        }
        src_is_a = !src_is_a;
    }
    *data = if src_is_a { buf_a } else { buf_b };
}

pub fn scatter_parallel(data: &[u32], smap: &ScatterMap, scratch: &mut [u32], shift: u32) {
    let scratch_ptr = scratch.as_mut_ptr() as usize;
    let thread_offsets = &smap.thread_offsets;
    data.par_chunks(CHUNK_SIZE).enumerate().for_each(|(t, chunk)| {
        let mut ptrs = thread_offsets[t];
        let out = unsafe { std::slice::from_raw_parts_mut(scratch_ptr as *mut u32, scratch.len()) };
        for &val in chunk {
            let b = ((val >> shift) & 0xFF) as usize;
            unsafe { *out.get_unchecked_mut(ptrs[b] as usize) = val; }
            ptrs[b] += 1;
        }
    });
}

#[inline(always)]
pub fn needed_passes(max_val: u32) -> usize {
    if max_val == 0 { return 1; }
    let bits = 32 - max_val.leading_zeros() as usize;
    (bits + 7) / 8
}

pub fn sort_with_options(data: &mut Vec<u32>, opts: SortOptions) {
    let size = data.len();
    if size < 2 { return; }
    let max_val = opts.known_max.unwrap_or_else(|| sample_max(data));
    let passes  = opts.force_passes.unwrap_or_else(|| needed_passes(max_val));

    match opts.strategy {
        SortStrategy::CountingU8 => counting_sort_u8(data),
        SortStrategy::RadixParallel => radix_parallel_n_passes(data, passes),
        _ => {
            let use_adaptive = !matches!(opts.strategy, SortStrategy::RadixSerial);
            execute_sapri_sort(data, passes, use_adaptive, 0, size);
        }
    }
}

pub fn sort_with_progress<F>(data: &mut Vec<u32>, opts: SortOptions, mut on_progress: F)
where F: FnMut(usize) {
    let size = data.len();
    if size < 2 { on_progress(100); return; }
    let max_val = opts.known_max.unwrap_or_else(|| sample_max(data));
    let passes  = opts.force_passes.unwrap_or_else(|| needed_passes(max_val));
    
    let mut scratch = vec![0u32; size];
    let mut src_is_data = true;

    for i in 0..passes {
        let shift = (i * 8) as u32;
        let (src, dst) = if src_is_data { (&data[..], &mut scratch[..]) } else { (&scratch[..], &mut data[..]) };
        
        let mut count = phase_1_serial(src, shift);
        let mut total = 0usize;
        for c in count.iter_mut() { let old = *c; *c = total; total += old; }
        
        for (idx, &val) in src.iter().enumerate() {
            let b = ((val >> shift) & 0xFF) as usize;
            unsafe {
                *dst.get_unchecked_mut(count[b]) = val;
                count[b] += 1;
            }
            if idx & PROGRESS_MASK == 0 { on_progress((i * 100 / passes) + (idx * 100 / (size * passes))); }
        }
        src_is_data = !src_is_data;
    }
    if !src_is_data { data.copy_from_slice(&scratch); }
    on_progress(100);
}
