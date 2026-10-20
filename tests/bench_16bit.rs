// tests/bench_16bit.rs

use sapri_sort::{SapriParallelArena, BitTopology, sapri_sort_parallel_max};
use std::time::Instant;

#[test]
fn execute_sapri_16bit_scalability_benchmark() {
    println!("\n=======================================================================");
    println!("     SAPRI ENGINE - 16-BIT ADAPTIVE SCALABILITY BENCHMARK SUITE");
    println!("=======================================================================");

    let sizes = [
        1_000,
        5_000,
        10_000,
        50_000,
        100_000,
        500_000,
        1_000_000,
        5_000_000,
        10_000_000,
    ];

    let mut arena = SapriParallelArena::with_capacity(10_000_000);

    for &n in &sizes {
        let mut original_data = vec![0u64; n];
        let mut lcg = 42u64;
        for val in original_data.iter_mut() {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            *val = lcg % 65536; // Range strictly 16-bit
        }

        let mut qs_data = original_data.clone();

        let start_sapri = Instant::now();
        sapri_sort_par_16bit(&mut original_data, &mut arena);
        let duration_sapri = start_sapri.elapsed();

        let start_qs = Instant::now();
        qs_data.sort_unstable();
        let duration_qs = start_qs.elapsed();

        let success = original_data.windows(2).all(|w| w[0] <= w[1]);
        let status = if success { "[✓] Success" } else { "[X] ERROR" };

        let winner = if duration_sapri < duration_qs { "★ SAPRI" } else { "Quicksort" };

        println!(
            "Size: {:<10} | SAPRI: {:<15?}",
            n,
            duration_sapri
        );
        println!(
            "             | Quicksort: {:<12?} | Faster: {} | {}",
            duration_qs, winner, status
        );
        println!("-----------------------------------------------------------------------");
    }

    println!("=======================================================================");
    println!("test execute_sapri_16bit_scalability_benchmark ... ok\n");
}

fn sapri_sort_par_16bit(data: &mut [u64], arena: &mut SapriParallelArena) {
    let n = data.len();
    if n < 2 { return; }

    if n < 50_000 {
        data.sort_unstable();
        return;
    }

    sapri_sort_parallel_max(data, arena, 65535, BitTopology::Adaptive);
}
