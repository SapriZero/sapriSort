// tests/bench_signed_scalability.rs

use std::time::Instant;
use rand::Rng;

// Pulling signed sort interfaces and initialization hooks from the public crate root
use sapri_sort::{sapri_sort_i64, init_parallel_engine};

/// Data generation strategies for the full signed i64 spectrum
#[derive(Debug, Clone, Copy)]
enum SignedDataType {
    PureRandomSigned,
    ConcentratedNegative,
    GroupedSpanningZero,
    Max90NegativeOnBlock,
}

#[test]
fn execute_sapri_signed_scalability_benchmark() {
    // Initialize the parallel thread pool (0 = automatic configuration based on core count)
    init_parallel_engine(0);

    println!("\n=======================================================================");
    println!("       SAPRI ENGINE - SIGNED i64 SCALABILITY BENCHMARK SUITE");
    println!("=======================================================================");

    // Array data sizes ranging from 1 element up to 10M elements
    let sizes = vec![1, 15, 100, 1000, 10000, 100000, 1000000, 10000000];

    // Execution list of signed target strategies
    let types_to_test = vec![
        SignedDataType::PureRandomSigned,
        SignedDataType::ConcentratedNegative,
        SignedDataType::GroupedSpanningZero,
        SignedDataType::Max90NegativeOnBlock,
    ];

    for data_type in types_to_test {
        println!("\n>>> [ TARGET SIGNED TEST STRATEGY: {:?} ] <<<", data_type);
        
        for &size in &sizes {
            execute_single_signed_comparison(size, data_type);
        }
        println!("-----------------------------------------------------------------------");
    }
    
    println!("=======================================================================");
}

/// Allocates, populates, runs timers and verifies data integrity on signed datasets
fn execute_single_signed_comparison(size: usize, data_type: SignedDataType) {
    let mut rng = rand::thread_rng();
    let mut data_sapri = vec![0i64; size];

    // 1. SIGNED DATA GENERATION LOGIC
    match data_type {
        SignedDataType::PureRandomSigned => {
            for i in 0..size {
                data_sapri[i] = rng.gen::<i64>();
            }
        }
        SignedDataType::ConcentratedNegative => {
            // Highly concentrated clusters deep inside the negative boundaries
            let base = -5_000_000_000i64;
            for i in 0..size {
                data_sapri[i] = base + (rng.gen::<u32>() % 25_000) as i64;
            }
        }
        SignedDataType::GroupedSpanningZero => {
            // Clusters containing a mix of positive values, negative values, and zeros
            let mut block_base = 0i64;
            for i in 0..size {
                if i % 1000 == 0 {
                    block_base = rng.gen::<i64>();
                }
                data_sapri[i] = block_base.wrapping_add((rng.gen::<i8>() as i64) % 15);
            }
        }
        SignedDataType::Max90NegativeOnBlock => {
            // 90% of data localized within tight negative limits, 10% completely wild
            for i in 0..size {
                if rng.gen_bool(0.90) {
                    data_sapri[i] = -((rng.gen::<u16>()) as i64);
                } else {
                    data_sapri[i] = rng.gen::<i64>();
                }
            }
        }
    }

    // Duplicate array state for standard unstable comparison verification
    let mut data_quicksort = data_sapri.clone();

    // -----------------------------------------------------------------
    // RUNTIME RUN 1: SAPRI SIGNED ENGINE (Via Bit-Flipping Interface)
    // -----------------------------------------------------------------
    let start_sapri = Instant::now();
    sapri_sort_i64(&mut data_sapri); 
    let duration_sapri = start_sapri.elapsed();

    // -----------------------------------------------------------------
    // RUNTIME RUN 2: STANDARD QUICKSORT (Unstable i64)
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
        "Size: {:<10} | SAPRI (i64): {:<12?} | Quicksort: {:<12?} | {}",
        format_thousand_separator(size),
        duration_sapri,
        duration_quick,
        status
    );
}

/// Helper function to output clean, readable numbers using international notation
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
