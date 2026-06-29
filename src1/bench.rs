
// use crate::sapri_sort::{sapri_sort, sapri_radix_sort, sapri_curried};
use sapri_sort::prove::{
    sort_with_progress,
};

use std::time::{Instant, Duration};

use crate::utils::{DataFactory, GenStrategy}; 

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
    // 1. Ciclo esterno: Strategia (per generare meno dati pesanti possibile)
    for strategy in &params.strategies {
        println!("\n--- Strategia di generazione: {:?} ---", strategy);
/*
         
        // Genera il set di dati gigante una sola volta per strategia
        let master_data = DataFactory::generate_by_strategy(10_000_000, 0, u32::MAX, strategy);

        // 2. Ciclo medio: Profondità
        for &depth in &params.byte_depths {
            let max_val = if depth >= 4 { 
                u32::MAX 
            } else { 
                ((1u64 << (depth * 8)) - 1) as u32 
            };

            // 3. Ciclo interno: Grandezza
            for &size in &params.sizes {
                if size > master_data.len() { continue; }

                // Preleva una fetta (slice) casuale
                let max_offset = master_data.len() - size;
                let offset = if max_offset > 0 { rand::random::<usize>() % max_offset } else { 0 };
                
                // Crea copia e restringe il range
                let data: Vec<u32> = master_data[offset..offset + size]
                    .iter()
                    .map(|&val| val & max_val) 
                    .collect();

                println!(">>> Depth: {}, Size: {}", depth, size);
                
                // --- LOGICA DI BENCHMARK (Recuperata dalla vecchia versione) ---
                let mut results = Vec::new();
                
                // Benchmark SAPRI
                results.push(benchmark_algorithm("SAPRI", data.clone(), |d| {
                    sapri_sort(d, depth);
                }));

                // Benchmark Quicksort
                results.push(benchmark_algorithm("Quicksort", data.clone(), |d| {
                    d.sort_unstable();
                }));

                // Validazione
                let reference = &results[0].sorted_data;
                for res in &results {
                    if &res.sorted_data != reference {
                        eprintln!("[!] ERRORE: {} ha prodotto risultati diversi!", res.name);
                    }
                }
            }
        }
        */
    } 
}

pub fn benchmark_algorithm<F>(name: &str, mut data: Vec<u32>, mut sort_fn: F) -> SortResult 
where F: FnMut(&mut [u32])
{
    let start = Instant::now();
    
    // Esecuzione protetta per evitare crash totali del benchmark
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sort_fn(&mut data);
    }));
    
    let duration = start.elapsed();
    
    if result.is_err() {
        eprintln!("[!] PANIC: {} ha fallito!", name);
    } else {
        println!("{}: {:?}", name, duration);
    }
    
    SortResult {
        name: name.to_string(),
        duration,
        sorted_data: data,
    }
}
