mod utils; // Importa il file util.rs
use crate::utils::{GenStrategy}; 
mod bench;
use crate::bench::{run_benchmark_suite, BenchParams};


fn main() {
    let params = BenchParams {
        // sizes: vec![10_000, 100_000, 1_000_000],
        sizes: vec![10_000_000],
        // sizes: vec![100_000, 1_000_000],
        // sizes: vec![10, 100, 1000, 10_000],
        // sizes: vec![10, 30, 50, 80],
        // byte_depths: vec![1, 2, 4],
        byte_depths: vec![4],
        strategies: vec![GenStrategy::Grouped, GenStrategy::Concentrated, GenStrategy::Max90OnBlock],
    };

    run_benchmark_suite(params);
}
