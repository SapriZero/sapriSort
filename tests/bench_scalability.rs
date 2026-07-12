// tests/bench_scalability.rs

use std::time::Instant;
use rand::Rng;

// Standardized imports pulling directly from the public crate root
use sapri_sort::{sapri_sort_unstable_max, init_parallel_engine};

/// Data generation strategies matching our global English layout
#[derive(Debug, Clone, Copy)]
enum DataType {
    PureRandom,
    Concentrated,
    Grouped,
    Max90OnBlock,
}

#[test]
fn execute_sapri_scalability_benchmark() {
    // Initialize the parallel thread pool (0 = automatic configuration based on core count)
    init_parallel_engine(0);

    println!("\n=======================================================================");
    println!("        SAPRI ENGINE - ADAPTIVE SCALABILITY BENCHMARK SUITE");
    println!("=======================================================================");

    // Array data sizes ranging from 1 element up to 10M elements
    let sizes = vec![1, 15, 100, 1000, 10000, 100000, 1000000, 10000000];

    // Execution list of target strategies
    let types_to_test = vec![
        DataType::PureRandom,
        DataType::Concentrated,
        DataType::Grouped,
        DataType::Max90OnBlock,
    ];

    for data_type in types_to_test {
        println!("\n>>> [ TARGET TEST STRATEGY: {:?} ] <<<", data_type);
        
        for &size in &sizes {
            execute_single_comparison(size, data_type);
        }
        println!("-----------------------------------------------------------------------");
    }
    
    println!("=======================================================================");
}

/// Orchestrator: Allocates, clones, runs timers and checks sorting results
fn execute_single_comparison(size: usize, data_type: DataType) {
    let mut rng = rand::thread_rng();
    let mut data_sapri = vec![0u64; size];

    // 1. DATA GENERATION LOGIC
    match data_type {
        DataType::PureRandom => {
            for i in 0..size {
                data_sapri[i] = rng.gen::<u64>();
            }
        }
        DataType::Concentrated => {
            let base = u32::MAX as u64 + 500_000;
            for i in 0..size {
                data_sapri[i] = base + (rng.gen::<u32>() % 10_000) as u64;
            }
        }
        DataType::Grouped => {
            let mut block_value = 0u64;
            for i in 0..size {
                if i % 1000 == 0 {
                    block_value = rng.gen::<u64>();
                }
                data_sapri[i] = block_value.wrapping_add((rng.gen::<u8>() % 10) as u64);
            }
        }
        DataType::Max90OnBlock => {
            for i in 0..size {
                if rng.gen_bool(0.90) {
                    data_sapri[i] = (rng.gen::<u16>()) as u64;
                } else {
                    data_sapri[i] = rng.gen::<u64>();
                }
            }
        }
    }

    // 2. DYNAMIC MAX VALUE DETECTION
    // Finds the exact maximum element to optimize passes and secure inner matrix slicing bounds.
    // If the array is empty, defaults safely to 0.
    let max_val = data_sapri.iter().max().cloned().unwrap_or(0);

    // Duplicate array state for standard Quicksort comparison
    let mut data_quicksort = data_sapri.clone();

    // -----------------------------------------------------------------
    // RUNTIME RUN 1: SAPRI HYBRID ENGINE
    // -----------------------------------------------------------------
    let start_sapri = Instant::now();
    sapri_sort_unstable_max(&mut data_sapri, max_val); 
    let duration_sapri = start_sapri.elapsed();

    // -----------------------------------------------------------------
    // RUNTIME RUN 2: STANDARD QUICKSORT (Unstable)
    // -----------------------------------------------------------------
    let start_quick = Instant::now();
    data_quicksort.sort_unstable();
    let duration_quick = start_quick.elapsed();

    // -----------------------------------------------------------------
    // DATA INTEGRITY VALIDATION
    // -----------------------------------------------------------------
    let correct = data_sapri == data_quicksort;
    let status = if correct { "[✓] Success" } else { "[X] INTEGRITY CRITICAL ERROR" };
    
    println!(
        "Size: {:<10} | SAPRI: {:<12?} | Quicksort: {:<12?} | {}",
        format_thousand_separator(size),
        duration_sapri,
        duration_quick,
        status
    );
}

/// Helper function to output clean, readable numbers using international notation (e.g., 10,000,000)
fn format_thousand_separator(mut num: usize) -> String {
    if num == 0 { return "0".to_string(); }
    let mut s = String::new();
    let mut count = 0;
    while num > 0 {
        if count == 3 {
            s.insert(0, ',');
            count = 0;
        }
        s.insert(0, std::char::from_digit((num % 10) as u32, 10).unwrap());
        num /= 10;
        count += 1;
    }
    s
}
