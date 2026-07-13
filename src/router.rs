// src/router.rs

//! # SAPRI Hybrid Engine Router Module
//!
//! This module provides the core adaptive routing mechanics, thread-local
//! scratchpad buffers, and deterministic sorting micro-engines for the SAPRI architecture.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::cell::RefCell;
use thread_local::ThreadLocal;

// --- Constants and Telemetry ---

/// Global atomic counter tracking the number of adaptive router invocations.
static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Thread-safe localized vector capturing runtime partition execution telemetry.
static BUCKET_LOGS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// The maximum number of localized bucket events to record in telemetry.
const MAX_LOG_BUCKETS: usize = 40;

/// Critical data size threshold below which parallel task distribution is bypassed
/// to prevent hardware thread scheduling overhead on thin workloads.
const PARALLEL_CUTOFF: usize = 2048;

/// Strategic bit-slice execution topologies used to configure macro radix pass sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitTopology {
    /// Standard execution sequence running four consecutive 8-bit passes.
    Standard8x4,
    /// Accelerated configuration processing a 16-bit leading pass followed by two 8-bit passes.
    Pass16x8x8,
    /// Balanced configuration running an 8-bit macro pass, a 16-bit intermediate accelerator, and an 8-bit leaf.
    Pass8x16x8,
    /// Closing accelerator configuration running two 8-bit macro passes followed by a 16-bit leaf.
    Pass8x8x16,
    /// Fully dynamic execution path evaluating data shapes at runtime via min/max metadata scanning.
    Adaptive,
}

/// Thread-persistent, reusable lookup and tracking scratchpad eliminating runtime allocation overhead.
pub struct ThreadScratchpad {
    /// Micro frequency table for 4-bit passes, perfectly fitting a single 64-byte L1 cache line.
    pub count_4: Vec<u32>,
    /// Highly localized 8-bit frequency table.
    pub count_8: Vec<u32>,
    /// Dense 16-bit frequency lookup table.
    pub count_16: Vec<u32>,
    /// Tracking buffer capturing modified indices.
    pub active_indices: Vec<usize>,
}

impl ThreadScratchpad {
	///
    pub fn new() -> Self {
        Self {
            count_4: vec![0; 16], // 16 elementi * 4 byte = 64 byte totali!
            count_8: vec![0; 256],
            count_16: vec![0; 65536],
            active_indices: Vec::with_capacity(1024),
        }
    }

    /// Flushes modified counters inside the 16-bit tracking array using the active sparse index record.
    #[inline(always)]
    pub fn sparse_reset_16(&mut self) {
        for &idx in &self.active_indices {
            self.count_16[idx] = 0;
        }
        self.active_indices.clear();
    }
}

impl Default for ThreadScratchpad {
    fn default() -> Self {
        Self::new()
    }
}

// --- Isolated Helper Functions ---

/// Non-blocking telemetry function logging partition lengths into the diagnostic store.
#[inline(always)]
fn log_bucket_telemetry(n: usize) {
    let current_call = CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    if current_call < MAX_LOG_BUCKETS {
        if let Ok(mut logs) = BUCKET_LOGS.lock() {
            logs.push((current_call, n));
        }
    }
}

/// Mathematical boundary metadata capturing the extreme metrics of an unaligned array block.
#[derive(Debug, Clone, Copy)]
pub struct BucketMetadata {
    /// True if the segment components match sequential ordering bounds.
    pub is_sorted: bool,
    /// Absolute minimum numerical boundary discovered within the collection slice.
    pub min: u64,
    /// Absolute maximum numerical boundary discovered within the collection slice.
    pub max: u64,
}
/// streaming min/max scanner.
/// Maintained compatibility with legacy structural fields while eliminating branching dependencies.
#[inline(always)]
pub fn analyze_bucket(data: &[u64]) -> BucketMetadata {
    let mut min = data[0];
    let mut max = data[0];

    // Il compilatore srotola questo ciclo ed esegue l'auto-vettorializzazione hardware
    for &val in data.iter().skip(1) {
        if val < min { min = val; }
        if val > max { max = val; }
    }

    BucketMetadata {
        is_sorted: false, // Forziamo false per delegare il workflow ai micro-engine branchless
        min,
        max,
    }
}

// --- Primary Flat State Dispatcher ---

/// Central dispatcher routing array blocks to specific hardware execution strategies
/// based on the chosen bit topology configuration and core memory density.

// --- Primary Flat State Dispatcher ---

/// Central dispatcher routing array blocks to specific hardware execution strategies
/// based on the chosen bit topology configuration and core memory density.
pub fn router(
    data: &mut [u64], 
    shift: u32, 
    topology: BitTopology, 
    pool: &ThreadLocal<RefCell<ThreadScratchpad>>
) {
    let n = data.len();
    if n <= 1 { return; }

    log_bucket_telemetry(n);

    // --- 1. Structural assessment (Fast streaming SIMD-friendly check) ---
    let meta = analyze_bucket(data);
    if meta.is_sorted || meta.min == meta.max { 
        return; 
    }

    // --- SHIFT ALIGNMENT CORRECTION ---
    // Se siamo al livello radice (shift == 0 ma spettro ampio), allineiamo lo shift 
    // al blocco di 4 bit più alto necessario per contenere il valore massimo reale.
    let mut effective_shift = if shift == 0 && meta.max > 0 {
        let total_bits = 64 - meta.max.leading_zeros();
        // Arrotondiamo per eccesso al prossimo blocco di 4 bit
        ((total_bits + 3) / 4) * 4
    } else {
        shift
    };

    // --- GLOBAL ADAPTIVE INTERCEPTION (Dynamic Bit-Width Zoom & Cutoff Bypass) ---
    if let BitTopology::Adaptive = topology {
        let delta = meta.max - meta.min;
        let required_bits = 64 - (delta | 1).leading_zeros();

        // ACCELERATOR ALPHA: Spettro <= 8-bit
        if required_bits <= 8 {
            let scratch_cell = pool.get_or(|| RefCell::new(ThreadScratchpad::new()));
            let mut scratch = scratch_cell.borrow_mut();
            counting_sort_u64_regenerative_8(data, meta.min, &mut scratch);
            return;
        }

        // ACCELERATOR BETA: Spettro <= 16-bit per blocchi piccoli
        if required_bits <= 16 && n < 16384 {
            let scratch_cell = pool.get_or(|| RefCell::new(ThreadScratchpad::new()));
            let mut scratch = scratch_cell.borrow_mut();
            counting_sort_u64_regenerative_16(data, meta.min, &mut scratch);
            return;
        }

        // Sintonizzazione ricorsiva dello shift
        if required_bits < effective_shift {
            effective_shift = (required_bits / 4) * 4;
        }
    }

    // --- 2. Workload scale throttling (Cut-off evaluation standard) ---
    if n < PARALLEL_CUTOFF {
        let scratch_cell = pool.get_or(|| RefCell::new(ThreadScratchpad::new()));
        let mut scratch = scratch_cell.borrow_mut();

        match (topology, effective_shift) {
            (BitTopology::Pass16x8x8, 16) | (BitTopology::Pass8x16x8, 16) | (BitTopology::Pass8x8x16, 16) => {
                let base_mask = data[0] & 0xFFFFFFFFFFFF0000;
                counting_sort_u64_regenerative_16(data, base_mask, &mut scratch);
            }
            _ => {
                if effective_shift >= 8 {
                    let base_mask = data[0] & 0xFFFFFFFFFFFFFF00;
                    counting_sort_u64_regenerative_8(data, base_mask, &mut scratch);
                } else {
                    data.sort_unstable();
                }
            }
        }
        return;
    }

    // --- 3. DENSE WORKLOAD RADIX COMPONENT (4-Bit / 16-Way L1 Partitioning) ---
    // --- 3. DENSE WORKLOAD RADIX COMPONENT (4-Bit / 16-Way Out-of-Place Partitioning) ---
    if effective_shift >= 4 {
        let next_shift = effective_shift - 4;
        let scratch_cell = pool.get_or(|| RefCell::new(ThreadScratchpad::new()));
        let mut scratch = scratch_cell.borrow_mut();
        
        // Fase 1: Istogramma delle frequenze
        for &val in data.iter() {
            let bucket = ((val >> next_shift) & 0x0F) as usize;
            scratch.count_4[bucket] += 1;
        }

        // Fase 2: Mappatura degli offset di partenza
        let mut offsets = [0usize; 16];
        let mut current_offset = 0;
        for i in 0..16 {
            offsets[i] = current_offset;
            current_offset += scratch.count_4[i] as usize;
        }

        // Fase 3: Allocazione temporanea locale per mantenere la linearità di scrittura della Cache
        // (Evita i Cache Miss devastanti dello swap in-place su grandi volumi)

        let mut temp_buffer = vec![0u64; n]; 
        for &val in data.iter() {
            let bucket = ((val >> next_shift) & 0x0F) as usize;
            temp_buffer[offsets[bucket]] = val;
            offsets[bucket] += 1;
        }

        data.copy_from_slice(&temp_buffer);
        // Riversiamo i dati ordinati per questo blocco nell'array originale
        data.copy_from_slice(&temp_buffer);

        // Calcoliamo i confini effettivi per la ricorsione usando gli offset finali
        let mut start = 0;
        for i in 0..16 {
            let end = offsets[i];
            if end > start + 1 {
                router(&mut data[start..end], next_shift, topology, pool);
            }
            start = end;
        }

        // Resettiamo lo scratchpad
        for cnt in scratch.count_4.iter_mut() { *cnt = 0; }
        return;
    }

    // --- TEST DI CADUTA: Se arriviamo qui, stiamo fallendo il routing dei bit ---
    panic!("ERRORE: SAPRI non è riuscito a intercettare i bit ed è caduto sul fallback finale!");
}


// --- Core Deterministic Sorting Engines ---

/// Performs linear, prefix-sum-less data distribution and hardware-optimized 
/// regeneration over an 8-bit lookup window using thread-persistent buffers.
pub fn counting_sort_u64_regenerative_8(data: &mut [u64], base_mask: u64, scratch: &mut ThreadScratchpad) {
    for &val in data.iter() { 
        scratch.count_8[(val & 0xFF) as usize] += 1; 
    }
    
    let mut pos = 0usize;
    for (val, cnt) in scratch.count_8.iter_mut().enumerate() {
        if *cnt > 0 {
            let reconstructed_val = base_mask | (val as u64);
            for _ in 0..*cnt { 
                data[pos] = reconstructed_val; 
                pos += 1; 
            }
            *cnt = 0; // Immediate zero-cost index scrubbing
        }
    }
}

/// Executes an accelerated 16-bit prefix-sum-less memory generation routine
/// utilizing active tracking records to perform targeted cache-friendly sparse resets.
pub fn counting_sort_u64_regenerative_16(data: &mut [u64], base_mask: u64, scratch: &mut ThreadScratchpad) {
    // Frequency collection pass accompanied by unique sparse tracking interception
    for &val in data.iter() {
        let idx = (val & 0xFFFF) as usize;
        if scratch.count_16[idx] == 0 {
            scratch.active_indices.push(idx);
        }
        scratch.count_16[idx] += 1;
    }
    
    let mut pos = 0usize;
    
    // Sort active indices to guarantee perfect linear write streaming in RAM
    scratch.active_indices.sort_unstable();
    
    for &idx in &scratch.active_indices {
        let cnt = scratch.count_16[idx];
        let reconstructed_val = base_mask | (idx as u64);
        for _ in 0..cnt {
            data[pos] = reconstructed_val;
            pos += 1;
        }
    }

    // Target reset routine clearing exclusively the modified memory slots
    scratch.sparse_reset_16();
}

/// Prints recorded partition size allocations and diagnostic metrics accumulated over benchmark executions.
pub fn print_bucket_telemetry() {
    if let Ok(logs) = BUCKET_LOGS.lock() {
        println!("=== SAPRI SUB-BUCKET TELEMETRY ANALYSIS (Top {}) ===", logs.len());
        for (id, size) in logs.iter() {
            println!("Leaf Node Block #{:02} -> Tracked Density: {} keys", id, size);
        }
        println!("==================================================");
    }
}
