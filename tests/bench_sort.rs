// tests/bench_sort.rs

use sapri_sort::{sapri_sort_max, init_parallel_engine};
use std::time::Instant;

#[path = "common/random_array.rs"]
mod random_array;
use random_array::{DataFactory, GenStrategy};

/// Verifies data integrity between the custom engine and the baseline results.
/// Returns the index and mismatching values if an error is encountered.
fn safe_check_correctness(sapri: &[u64], quick: &[u64]) -> Result<(), (usize, u64, u64)> {
    if sapri.len() != quick.len() {
        return Err((0, sapri.len() as u64, quick.len() as u64));
    }
    for i in 0..sapri.len() {
        if sapri[i] != quick[i] {
            return Err((i, sapri[i], quick[i])); // Returns the index of the first divergence
        }
    }
    Ok(())
}

#[test]
fn execute_full_hybrid_32_and_64_bit_benchmark() {
    let size = 10_000_000;
    let strategies = [
        GenStrategy::Grouped,
        GenStrategy::Concentrated,
        GenStrategy::Max90OnBlock,
    ];

    println!("\n=======================================================================");
    println!("   SAPRI HYBRID ENGINE BENCHMARK SUITE - 32 vs 64 BIT MATRIX EVALUATION");
    println!("=======================================================================");

    // =========================================================================
    // BLOCK 1: 32-BIT SCENARIOS (RAM Optimization & Compact Scratchpad)
    // =========================================================================
    println!("\n>>> [ MACRO BLOCK: 32-BIT DATA RANGE (max_val <= u32::MAX) ] <<<");
    
    // Forces Rayon to initialize using all available logical CPU worker cores
    init_parallel_engine(0); 
    
    let max_val_32 = u32::MAX as u64;

    for strategy in &strategies {
        println!("\n--- 32-bit Scenario | Strategy: {:?} ---", strategy);
        let master_data = DataFactory::generate_by_strategy(size, 0, max_val_32, strategy);

        let mut data_sapri = master_data.clone();
        let start_sapri = Instant::now();
        sapri_sort_max(&mut data_sapri, max_val_32);
        let duration_sapri = start_sapri.elapsed();
        println!("  -> SAPRI (Hybrid 32-bit):   {:?}", duration_sapri);

        let mut data_quick = master_data.clone();
        let start_quick = Instant::now();
        data_quick.sort_unstable();
        let duration_quick = start_quick.elapsed();
        println!("  -> Quicksort (Unstable):     {:?}", duration_quick);

        // Safe validation routine preventing noisy terminal panics
        match safe_check_correctness(&data_sapri, &data_quick) {
            Ok(_) => println!("  [✓] Accuracy verified."),
            Err((idx, s_val, q_val)) => {
                panic!("\n[!] SORTING CRITICAL ERROR in 32-bit scope at index [{}]: SAPRI returned {}, expected {} (Quicksort)", idx, s_val, q_val);
            }
        }
    }

    println!("\n-----------------------------------------------------------------------");

    // =========================================================================
    // BLOCK 2: 64-BIT SCENARIOS (Native Core Engine & Extended Scratchpad)
    // =========================================================================
    println!("\n>>> [ MACRO BLOCK: 64-BIT DATA RANGE (max_val > u32::MAX) ] <<<");
    
    // Configures max_val above u32::MAX boundaries to trigger native 64-bit routines,
    // keeping bounds conservative enough to maintain stable distribution metrics inside DataFactory.
    let max_val_64 = (u32::MAX as u64) + 100_000_000;

    for strategy in &strategies {
        println!("\n--- 64-bit Scenario | Strategy: {:?} ---", strategy);
        let master_data = DataFactory::generate_by_strategy(size, 0, max_val_64, strategy);

        let mut data_sapri = master_data.clone();
        let start_sapri = Instant::now();
        sapri_sort_max(&mut data_sapri, max_val_64);
        let duration_sapri = start_sapri.elapsed();
        println!("  -> SAPRI (Native 64-bit):   {:?}", duration_sapri);

        let mut data_quick = master_data.clone();
        let start_quick = Instant::now();
        data_quick.sort_unstable();
        let duration_quick = start_quick.elapsed();
        println!("  -> Quicksort (Unstable):     {:?}", duration_quick);

        // Safe validation routine preventing noisy terminal panics
        match safe_check_correctness(&data_sapri, &data_quick) {
            Ok(_) => println!("  [✓] Accuracy verified."),
            Err((idx, s_val, q_val)) => {
                panic!("\n[!] SORTING CRITICAL ERROR in 64-bit scope at index [{}]: SAPRI returned {}, expected {} (Quicksort)", idx, s_val, q_val);
            }
        }
    }

    println!("\n=======================================================================");
}
