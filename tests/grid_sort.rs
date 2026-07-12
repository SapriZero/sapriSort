// tests/grid_sort.rs

use sapri_sort::grid_sort as grid_module;

#[test]
fn micro_engines_comparative_benchmark() {
    use std::time::Instant;
    use rand::Rng;
    
    let sizes = [2, 3, 5, 10, 15, 32, 100, 500, 1000];
    let mut rng = rand::thread_rng();

    println!("\n=======================================================================");
    println!("        MICRO-SORT ENGINES COMPARATIVE BENCHMARK (N <= 1000)");
    println!("=======================================================================");
    println!("{:<8} | {:<12} | {:<12} | {:<12} | {:<12}", "Size", "GRID (Hybrid)", "Insertion", "Selection", "Quicksort");
    println!("-----------------------------------------------------------------------");

    for &size in sizes.iter() {
        // Genera array base casuale
        let base_data: Vec<u64> = (0..size).map(|_| rng.gen_range(0..100_000)).collect();
        const ITERATIONS: u32 = 2000;

        // 1. Test Grid Sort (Hybrid Branchless Network)
        let mut t_grid = std::time::Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut clone = base_data.clone();
            let start = Instant::now();
            grid_module::grid_sort(&mut clone);
            t_grid += start.elapsed();
        }
        
        // 2. Test Insertion Sort
        let mut t_insert = std::time::Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut clone = base_data.clone();
            let start = Instant::now();
            grid_module::grid_insertion_sort(&mut clone);
            t_insert += start.elapsed();
        }
        
        // 3. Test Selection Sort
        let mut t_select = std::time::Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut clone = base_data.clone();
            let start = Instant::now();
            grid_module::grid_selection_sort(&mut clone);
            t_select += start.elapsed();
        }
        
        // 4. Test Quicksort
        let mut t_quick = std::time::Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut clone = base_data.clone();
            let start = Instant::now();
            grid_module::grid_quicksort(&mut clone);
            t_quick += start.elapsed();
        }
    
        let avg_grid = t_grid / ITERATIONS;
        let avg_insert = t_insert / ITERATIONS;
        let avg_select = t_select / ITERATIONS;
        let avg_quick = t_quick / ITERATIONS;

        println!(
            "{:<8} | {:<12?} | {:<12?} | {:<12?} | {:<12?}",
            size, avg_grid, avg_insert, avg_select, avg_quick
        );
    }
    println!("=======================================================================\n");
}
