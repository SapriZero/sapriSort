// tests/bench_100m.rs

use std::time::Instant;
use sapri_sort::BitTopology;

// Import direct from the primary library API
use sapri_sort::{
    init_parallel_engine, 
    sapri_sort_parallel_in_arena, 
    SapriParallelArena,
};

// Assuming GenStrategy and DataFactory are exposed by your library crate.
// Adjust the path (e.g., `use crate::...` or `use sapri_sort::factory::...`) 
// if they are located within a specific submodule.
#[path = "common/random_array.rs"]
mod random_array;
use random_array::{DataFactory, GenStrategy};

#[test]
fn execute_sapri_100m_monolithic_benchmark() {
    // Initializes the global parallel thread pool (0 = auto-detect based on physical cores)
    init_parallel_engine(0);

    println!("\n=======================================================================");
    println!("   SAPRI PARALLEL ENGINE - 100M ZERO-ALLOCATION MATRIX BENCHMARK");
    println!("=======================================================================");
    
    // Monolithic sample size set to 100 Million elements
    let size = 100_000_000;
    
    // 1. ARENA PRE-ALLOCATION AND WARM-UP (Enforces Zero Page Faults during hot timing paths)
    println!("Pre-allocating and warming up Parallel Arena (800 MB)...");
    let mut arena = SapriParallelArena::with_capacity(size);

    // =========================================================================
    //   STRATEGY CONFIGURATION ARRAY
    //   Uncomment or comment items here to control which benchmarks execute.
    // =========================================================================
    let strategies_to_run = vec![
        // GenStrategy::Grouped,
        // GenStrategy::GroupedUnordered,
        // GenStrategy::Random,
        // GenStrategy::Concentrated,
        GenStrategy::Max90OnBlock,
    ];
    
    for strategy in strategies_to_run {
        println!("\nAllocating data vector for {:?} (800 MB)...", strategy);
        
        // Setup specialized evaluation parameters based on target profiles
        let max_boundary = match strategy {
            GenStrategy::Grouped | GenStrategy::GroupedUnordered => 1000,
            GenStrategy::Concentrated => (u32::MAX - 100_000) as u64 + 50_000,
            GenStrategy::Max90OnBlock | GenStrategy::Random => u32::MAX as u64,
        };

        // 2. DATA GENERATION VIA FACTORY (Keeps code DRY and eliminates dead_code warnings)
        let mut data_sapri = DataFactory::generate_by_strategy(size, 0, max_boundary, &strategy);
        
        println!("Data generation completed. Cloning array for Quicksort baseline...");
        let mut data_quicksort = data_sapri.clone();

        println!("\nRunning PARALLEL benchmark for strategy: {:?}", strategy);
        println!("-----------------------------------------------------------------------");

        // --- TIMING SAPRI ENGINE ---
        let start_sapri = Instant::now();
        
        // The library now scans the dataset and executes internal optimizations autonomously
		sapri_sort_parallel_in_arena(&mut data_sapri, &mut arena, BitTopology::Adaptive);
        let duration_sapri = start_sapri.elapsed();
        println!("-> SAPRI (Zero-Alloc Parallel): {:?}", duration_sapri);

        // --- TIMING STANDARD QUICKSORT ---
        let start_quick = Instant::now();
        data_quicksort.sort_unstable();
        let duration_quick = start_quick.elapsed();
        println!("-> Quicksort (Standard Unstable): {:?}", duration_quick);

        // --- RIGID DATA INTEGRITY VALIDATION ---
        print!("Validating sort accuracy... ");
        let correct = data_sapri == data_quicksort;
        if correct {
            println!("[✓] Success (Data Integrity Verified)");
        } else {
            println!("[X] INTEGRITY CRITICAL ERROR!");
        }
        println!("-----------------------------------------------------------------------");
    }
    
    println!("=======================================================================");
}
