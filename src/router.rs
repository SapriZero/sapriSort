// src/router.rs

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

// --- Constants and Telemetry ---

/// Global counter tracking the number of adaptive router invocations.
static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Thread-safe storage capturing execution telemetry metadata for analysis.
static BUCKET_LOGS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// The maximum number of localized bucket interactions to intercept and log.
const MAX_LOG_BUCKETS: usize = 40;

/// Execution strategies for leaf bucket sorting and data regeneration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmallBucketStrategy {
    /// Native unstable quicksort fallback path.
    Quicksort,
    /// Linear prefix-sum-less data regeneration using an 8-bit lookup table.
    Counting8,
    /// Linear prefix-sum-less data regeneration using a 16-bit lookup table.
    Counting16,
}

/// The globally active sorting strategy optimized for mid-range leaf buckets.
const ACTIVE_STRATEGY: SmallBucketStrategy = SmallBucketStrategy::Counting8;

// --- Isolated Helper Functions ---

#[inline(always)]
fn log_bucket_telemetry(n: usize) {
    let current_call = CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    if current_call < MAX_LOG_BUCKETS {
        if let Ok(mut logs) = BUCKET_LOGS.lock() {
            logs.push((current_call, n));
        }
    }
}

/// Classifications defining partition scale layout states.
#[derive(PartialEq)]
enum BucketSizeState {
    /// Ultra-small array configuration fitting inside standard L1 data cache bounds.
    Micro,
    /// Optimal density window where specialized memory regeneration functions thrive.
    SweetSpot,
    /// Oversized workload block exceeding maximum hardware cache line capacity.
    Macro,
}

#[inline(always)]
fn classify_size(n: usize) -> BucketSizeState {
    if n <= 128 {
        BucketSizeState::Micro
    } else if n <= 10000 {
        BucketSizeState::SweetSpot
    } else {
        BucketSizeState::Macro
    }
}

// --- Primary Flat State Dispatcher ---

/// Dispatches small or localized sub-buckets to specialized sorting algorithms
/// based on the composite state of partition density and remaining radix bits.
pub fn router(data: &mut [u64], shift: u32) {
    let n = data.len();
    if n <= 1 { return; }

    log_bucket_telemetry(n);

    let size_state = classify_size(n);

    // Flattened decision matrix executing branchless-style state selection.
    match (size_state, shift, ACTIVE_STRATEGY) {
        
        // State 1: Micro workloads are processed natively within hardware registers.
        (BucketSizeState::Micro, _, _) => data.sort_unstable(),

        // State 2: Mid-range array encountering final 16-bit remaining range pass.
        (BucketSizeState::SweetSpot, 16, SmallBucketStrategy::Counting16) => {
            let base_mask = data[0] & 0xFFFFFFFFFFFF0000;
            counting_sort_u64_regenerative_16(data, base_mask);
        }

        // State 3: Target sub-leaf matching final 8-bit block under strict cache constraints.
        (BucketSizeState::SweetSpot, 8, SmallBucketStrategy::Counting8) if n <= 512 => {
            let base_mask = data[0] & 0xFFFFFFFFFFFFFF00;
            counting_sort_u64_regenerative_8(data, base_mask);
        }

        // Default State: Fallback execution path for sparse macro-blocks.
        _ => data.sort_unstable(),
    }
}

// --- Core Deterministic Sorting Engines ---
/// 
fn counting_sort_u64_regenerative_8(data: &mut [u64], base_mask: u64) {
    let mut count = [0u32; 256];
    for &val in data.iter() { count[(val & 0xFF) as usize] += 1; }
    
    let mut pos = 0usize;
    for (val, &cnt) in count.iter().enumerate() {
        let reconstructed_val = base_mask | (val as u64);
        for _ in 0..cnt { 
            data[pos] = reconstructed_val; 
            pos += 1; 
        }
    }
}
/// 
fn counting_sort_u64_regenerative_16(data: &mut [u64], base_mask: u64) {
    let mut count = [0u32; 65536];
    for &val in data.iter() { count[(val & 0xFFFF) as usize] += 1; }
    
    let mut pos = 0usize;
    for (val, &cnt) in count.iter().enumerate() {
        let reconstructed_val = base_mask | (val as u64);
        for _ in 0..cnt { 
            data[pos] = reconstructed_val; 
            pos += 1; 
        }
    }
}

/// Emits recorded partition size distributions and diagnostics gathered across benchmarking segments.
pub fn print_bucket_telemetry() {
    if let Ok(logs) = BUCKET_LOGS.lock() {
        println!("=== SAPRI SUB-BUCKET TELEMETRY ANALYSIS (Top {}) ===", logs.len());
        for (id, size) in logs.iter() {
            println!("Leaf Node Block #{:02} -> Tracked Density: {} keys", id, size);
        }
        println!("==================================================");
    }
}
