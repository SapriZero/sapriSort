// src/prove.rs
// Motore di sort SAPRI — libreria pura.
// Nessuna dipendenza da utils, rand, o altri moduli interni.

use rayon::prelude::*;

pub const CHUNK_SIZE:         usize = 128 * 1024;
pub const PROGRESS_STEP:      usize = 1 << 17;
pub const PROGRESS_MASK:      usize = PROGRESS_STEP - 1;
pub const PARALLEL_THRESHOLD: usize = 2_000_000;
pub const TINY: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortStrategy {
    Auto,
    CountingU8,
    RadixSerial,
    RadixParallel,
    RadixAdaptive,
}

#[derive(Debug, Clone, Copy)]
pub struct SortOptions {
    pub strategy:     SortStrategy,
    pub known_max:    Option<u32>,
    pub force_passes: Option<usize>,
}

impl Default for SortOptions {
    fn default() -> Self {
        Self { strategy: SortStrategy::Auto, known_max: None, force_passes: None }
    }
}

#[inline(always)]
pub fn needed_passes(max_val: u32) -> usize {
    if max_val == 0 { return 1; }
    let bits = 32 - max_val.leading_zeros() as usize;
    (bits + 7) / 8
}

#[inline]
pub fn sample_max(data: &[u32]) -> u32 {
    let step = (data.len() / 1024).max(1);
    data.iter().step_by(step).cloned().max().unwrap_or(0)
}

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

pub struct ScatterMap {
    pub thread_offsets: Vec<[u32; 256]>,
    pub global_offsets: [u32; 256],
}

pub fn compute_scatter_map(local_hists: &[[u32; 256]]) -> ScatterMap {
    let num_threads = local_hists.len();
    let mut totals = [0u32; 256];
    for hist in local_hists {
        for b in 0..256 { totals[b] += hist[b]; }
    }
    let mut global_offsets = [0u32; 256];
    let mut running = 0u32;
    for b in 0..256 {
        global_offsets[b] = running;
        running += totals[b];
    }
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

pub fn scatter_parallel(data: &[u32], smap: &ScatterMap, scratch: &mut [u32], shift: u32) {
    let scratch_addr   = scratch.as_mut_ptr() as usize;
    let scratch_len    = scratch.len();
    let thread_offsets = &smap.thread_offsets;
    data.par_chunks(CHUNK_SIZE)
        .enumerate()
        .for_each(|(t, chunk)| {
            let mut ptrs = thread_offsets[t];
            let out = unsafe {
                std::slice::from_raw_parts_mut(scratch_addr as *mut u32, scratch_len)
            };
            for &val in chunk {
                let b = ((val >> shift) & 0xFF) as usize;
                unsafe { *out.get_unchecked_mut(ptrs[b] as usize) = val; }
                ptrs[b] += 1;
            }
        });
}

#[inline]
pub fn scatter_serial(src: &[u32], dst: &mut [u32], count: &mut [usize; 256], shift: u32) {
    for &val in src.iter() {
        let b = ((val >> shift) & 0xFF) as usize;
        unsafe {
            *dst.get_unchecked_mut(*count.get_unchecked(b)) = val;
            *count.get_unchecked_mut(b) += 1;
        }
    }
}

#[inline]
pub fn scatter_serial_progress<F: FnMut(usize)>(
    src: &[u32], dst: &mut [u32], count: &mut [usize; 256], shift: u32, progress: &mut F,
) {
    for (i, &val) in src.iter().enumerate() {
        let b = ((val >> shift) & 0xFF) as usize;
        unsafe {
            *dst.get_unchecked_mut(*count.get_unchecked(b)) = val;
            *count.get_unchecked_mut(b) += 1;
        }
        if i & PROGRESS_MASK == 0 { progress(i); }
    }
}

// ============================================================
// INSERTION SORT — per bucket TINY (<= 4 elementi)
// ============================================================
#[inline]
fn insertion_sort(data: &mut [u32]) {
    for i in 1..data.len() {
        let key = data[i];
        let mut j = i;
        while j > 0 && data[j - 1] > key { data[j] = data[j - 1]; j -= 1; }
        data[j] = key;
    }
}

// ============================================================
// RADIX ADATTIVO A CASCATA
//
// Passata 0: tutto il buffer → istogramma → piano di lavoro
// Passate 1..N: solo sulle fette dei bucket Recurse
//
// Per ogni bucket dopo passata 0:
//   count == 0        → skip (Empty)
//   count <= TINY     → insertion sort inline (Tiny)
//   count == n        → tutti stesso byte → nessuno scatter (Uniform)
//   else              → radix sulla fetta [start..start+count] (Recurse)
//
// Il buffer dst dopo ogni scatter contiene fette ordinate per
// byte corrente — le fette non toccate restano intatte.
// ============================================================
// ============================================================
// RADIX ADATTIVO A CASCATA
//
// Struttura: Vec<Slice> delle fette attive per passata.
// Ogni passata costruisce la lista delle sotto-fette per la
// passata successiva — solo quelle con count > 1.
//
// Partial application: extract_byte viene costruita una volta
// per passata con shift fisso — il compilatore la inlinea come
// costante nella codegen.
//
// Invariante buffer: ogni fetta fa SEMPRE passes_total passate.
// Le fette Uniform (tutti stesso byte) fanno radix_pass_serial
// sulla fetta — nessun caso speciale che rompe l'alternanza.
// L'unica ottimizzazione safe è: bucket con count==1 non entrano
// in next_active (elemento singolo, già nella posizione finale).
// ============================================================
#[derive(Clone, Copy)]
struct Slice { start: usize, count: usize }

fn radix_adaptive(data: &mut Vec<u32>, passes: usize) {
    let n = data.len();
    if n < 2 { return; }
    let mut scratch = vec![0u32; n];
    let start_shift = (passes - 1) * 8;

    // "Currying" dello scratch: creiamo una closure che porta con sé 
    // il riferimento allo scratch, evitando di passarlo e gestirlo manualmente
    let mut msd_logic = |data_slice: &mut [u32], shift: i32| {
        msd_recursive(data_slice, &mut scratch, 0, n, shift);
    };

    msd_logic(data, start_shift as i32);
}

fn msd_recursive(data: &mut [u32], scratch: &mut [u32], start: usize, count: usize, shift: i32) {
    if count < 2 || shift < 0 { return; }

    // Se la fetta è piccola, l'Insertion Sort è imbattibile
    if count <= TINY {
        insertion_sort(&mut data[start..start + count]);
        return;
    }

    let s = shift as u32;
    let mut counts = [0usize; 256];
    let end = start + count;

    // 1. Istogramma
    for &val in &data[start..end] {
        counts[((val >> s) & 0xFF) as usize] += 1;
    }

    // 2. Prefix Sum (Offset locali alla fetta)
    let mut offsets = [0usize; 256];
    let mut current = 0usize;
    for i in 0..256 {
        offsets[i] = current;
        current += counts[i];
    }

    // 3. Scatter in scratch (temporaneo)
    let mut positions = offsets;
    for &val in &data[start..end] {
        let b = ((val >> s) & 0xFF) as usize;
        scratch[positions[b]] = val;
        positions[b] += 1;
    }

    // 4. Ricopia la fetta ordinata per questo byte nel buffer originale
    data[start..end].copy_from_slice(&scratch[0..count]);

    // 5. Ricorsione sui bucket che hanno più di un elemento
    for i in 0..256 {
        if counts[i] > 1 {
            // Scendiamo al byte successivo
            msd_recursive(data, scratch, start + offsets[i], counts[i], shift - 8);
        }
    }
}

// ============================================================
// BUILDING BLOCKS lineari
// ============================================================

fn counting_sort_u8(data: &mut [u32]) {
    let mut count = [0u32; 256];
    for &val in data.iter() { count[(val & 0xFF) as usize] += 1; }
    let mut pos = 0usize;
    for (val, &cnt) in count.iter().enumerate() {
        for _ in 0..cnt { data[pos] = val as u32; pos += 1; }
    }
}

#[inline]
fn radix_pass_serial(src: &[u32], dst: &mut [u32], shift: u32) {
    let mut count = [0usize; 256];
    for &val in src.iter() { count[((val >> shift) & 0xFF) as usize] += 1; }
    let mut total = 0usize;
    for c in count.iter_mut() { let old = *c; *c = total; total += old; }
    scatter_serial(src, dst, &mut count, shift);
}

fn radix_serial_n_passes(data: &mut Vec<u32>, passes: usize) {
    let n = data.len();
    let mut scratch = vec![0u32; n];
    let mut src_is_data = true;
    for i in 0..passes {
        let shift = (i * 8) as u32;
        if src_is_data { radix_pass_serial(data,    &mut scratch, shift); }
        else           { radix_pass_serial(&scratch, data,        shift); }
        src_is_data = !src_is_data;
    }
    if !src_is_data { data.copy_from_slice(&scratch); }
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
            let smap  = compute_scatter_map(&hists);
            scatter_parallel(&buf_a, &smap, &mut buf_b, shift);
        } else {
            let hists = phase_1_parallel(&buf_b, shift);
            let smap  = compute_scatter_map(&hists);
            scatter_parallel(&buf_b, &smap, &mut buf_a, shift);
        }
        src_is_a = !src_is_a;
    }
    *data = if src_is_a { buf_a } else { buf_b };
}

// ============================================================
// API PUBBLICA
// ============================================================

pub fn sort_orchestrator1(data: &mut Vec<u32>) {
    sort_with_options(data, SortOptions::default());
}
pub fn sort_orchestrator(data: &mut Vec<u32>) {
    let size = data.len();
    if size < 2 { return; }

    let max_val = sample_max(data);
    let passes = needed_passes(max_val);

    // Se i dati sono tanti, usiamo la logica parallela che hai già scritto
    if size >= PARALLEL_THRESHOLD {
        radix_parallel_n_passes(data, passes);
    } else {
        radix_serial_n_passes(data, passes);
    }
}


pub fn sort_with_options(data: &mut Vec<u32>, opts: SortOptions) {
    let size = data.len();
    if size < 2 { return; }
    let max_val = opts.known_max.unwrap_or_else(|| sample_max(data));
    let passes  = opts.force_passes.unwrap_or_else(|| needed_passes(max_val));
    match opts.strategy {
        SortStrategy::CountingU8    => counting_sort_u8(data),
        SortStrategy::RadixSerial   => radix_serial_n_passes(data, passes),
        SortStrategy::RadixParallel => radix_parallel_n_passes(data, passes),
        SortStrategy::RadixAdaptive => radix_adaptive(data, passes),
        SortStrategy::Auto => {
            if max_val <= 0xFF {
                counting_sort_u8(data);
            } else if size < 500_000 || passes <= 2 {
                radix_serial_n_passes(data, passes);
            } else if size < PARALLEL_THRESHOLD {
                radix_adaptive(data, passes);
            } else {
                radix_parallel_n_passes(data, passes);
            }
        }
    }
}

pub fn sort_with_progress<F>(data: &mut Vec<u32>, opts: SortOptions, mut on_progress: F)
where F: FnMut(usize) {
    let size = data.len();
    if size < 2 { on_progress(100); return; }
    let max_val = opts.known_max.unwrap_or_else(|| sample_max(data));
    let passes  = opts.force_passes.unwrap_or_else(|| needed_passes(max_val));
    let n = data.len();
    let mut scratch = vec![0u32; n];
    let mut src_is_data = true;
    for i in 0..passes {
        let shift = (i * 8) as u32;
        if src_is_data {
            let mut count = phase_1_serial(data, shift);
            let mut total = 0usize;
            for c in count.iter_mut() { let old = *c; *c = total; total += old; }
            scatter_serial_progress(data, &mut scratch, &mut count, shift, &mut on_progress);
        } else {
            let mut count = phase_1_serial(&scratch, shift);
            let mut total = 0usize;
            for c in count.iter_mut() { let old = *c; *c = total; total += old; }
            scatter_serial_progress(&scratch, data, &mut count, shift, &mut on_progress);
        }
        src_is_data = !src_is_data;
        on_progress((i + 1) * 100 / passes);
    }
    if !src_is_data { data.copy_from_slice(&scratch); }
    on_progress(100);
}

pub struct SortWorkspace {
    pub data:    Vec<u32>,
    pub scratch: Vec<u32>,
}

impl SortWorkspace {
    pub fn new(size: usize) -> Self {
        Self { data: (0..size as u32).rev().collect(), scratch: vec![0; size] }
    }
}
