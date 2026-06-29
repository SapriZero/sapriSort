// src/engine.rs

use crate::sapri_sort;
use crate::random_array::{DataFactory, GenStrategy};
use std::time::{Instant, Duration};

pub struct BenchParams {
    pub sizes: Vec<usize>,
    pub byte_depths: Vec<usize>,
    pub strategies: Vec<GenStrategy>,
}

pub struct SortResult {
    pub name: String,
    pub duration: Duration,
    pub sorted_data: Vec<u32>,
}

pub fn run_benchmark_suite(params: BenchParams) {
    for strategy in &params.strategies {
        println!("\n--- Strategia di generazione: {:?} ---", strategy);
         
        let master_data = DataFactory::generate_by_strategy(10_000_000, 0, u32::MAX, strategy);

        for &depth in &params.byte_depths {
            for &size in &params.sizes {
                if size > master_data.len() { continue; }
                
                let data = master_data[0..size].to_vec();
                println!("Taglio Array -> Size: {}, Depth: {}", size, depth);

                let mut results = Vec::new();
                
                // 1. Esecuzione SAPRI (Basato sulla logica prove2)
                results.push(benchmark_algorithm("SAPRI", data.clone(), |d| {
                    sapri_sort(d, depth);
                }));

                // 2. Esecuzione Quicksort standard di Rust
                results.push(benchmark_algorithm("Quicksort", data.clone(), |d| {
                    d.sort_unstable();
                }));

                // Validazione incrociata dei risultati
                let reference = &results[1].sorted_data;
                if results[0].sorted_data != *reference {
                    eprintln!("[!] ERRORE CRITICO: SAPRI ha corrotto l'array o non lo ha ordinato!");
                } else {
                    println!("[✓] Validazione Correttezza: OK");
                }
            }
        }
    } 
}

fn benchmark_algorithm<F>(name: &str, mut data: Vec<u32>, mut sort_fn: F) -> SortResult 
where F: FnMut(&mut [u32])
{
    let start = Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sort_fn(&mut data);
    }));
    let duration = start.elapsed();
    
    if result.is_err() {
        eprintln!("[!] PANIC: L'algoritmo {} è andato in crash!", name);
    } else {
        println!("  -> {}: {:?}", name, duration);
    }
    
    SortResult {
        name: name.to_string(),
        duration,
        sorted_data: data,
    }
}
