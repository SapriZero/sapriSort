// tests/bench_specialized_16bit.rs

use sapri_sort::{sapri_sort_16, sapri_sort_16_serial};
use std::time::Instant;

#[test]
fn execute_specialized_16bit_cut_benchmark() {
    println!("\n=======================================================================");
    println!("     SAPRI ENGINE - SPECIALIZED 16-BIT CUT BENCHMARK SUITE");
    println!("=======================================================================");

    let sizes = [
        100,
        1_000,
        10_000,
        100_000,
        1_000_000,
        10_000_000,
    ];

    for &n in &sizes {
        let mut original_data = vec![0u64; n];
        let mut lcg = 42u64;
        for val in original_data.iter_mut() {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            *val = lcg % 65536; // Range strictly 16-bit (0..65535)
        }

        let mut data_par = original_data.clone();
        let mut data_ser = original_data.clone();
        let mut data_qs = original_data.clone();

        // 1. SAPRI Parallel (sapri_sort_16)
        let start = Instant::now();
        sapri_sort_16(&mut data_par);
        let duration_par = start.elapsed();

        // 2. SAPRI Serial (sapri_sort_16_serial)
        let start = Instant::now();
        sapri_sort_16_serial(&mut data_ser);
        let duration_ser = start.elapsed();

        // 3. Standard Quicksort
        let start = Instant::now();
        data_qs.sort_unstable();
        let duration_qs = start.elapsed();

        // Verifica integrità
        let ok_par = data_par.windows(2).all(|w| w[0] <= w[1]);
        let ok_ser = data_ser.windows(2).all(|w| w[0] <= w[1]);
        let status = if ok_par && ok_ser { "[✓] Success" } else { "[X] ERROR" };

        println!("Size: {:<10}", n);
        println!("  ├─ SAPRI Parallel (16):   {:?}", duration_par);
        println!("  ├─ SAPRI Serial (16):     {:?}", duration_ser);
        println!("  └─ Quicksort (Std):       {:?} | Status: {}", duration_qs, status);
        println!("-----------------------------------------------------------------------");
    }

    println!("=======================================================================");
    println!("test execute_specialized_16bit_cut_benchmark ... ok\n");
}

#[test]
fn test_router_small_datasets_behavior() {
    use sapri_sort::sapri_sort;

    for &size in &[100, 1_000, 10_000, 100_000] {
        // Generiamo un set di dati di test con valori misti
        let mut data_sapri: Vec<u64> = (0..size).map(|i| (i * 37) % 65536).collect();
        let mut data_std = data_sapri.clone();

        // Eseguiamo SAPRI (router principale)
        let start = std::time::Instant::now();
        sapri_sort(&mut data_sapri);
        let duration_sapri = start.elapsed();

        // Eseguiamo Quicksort standard
        let start = std::time::Instant::now();
        data_std.sort_unstable();
        let duration_std = start.elapsed();

        assert_eq!(data_sapri, data_std, "Errore di ordinamento per size {}", size);
        
        println!(
            "Size: {:<8} | SAPRI Router: {:?} | Std Quicksort: {:?}",
            size, duration_sapri, duration_std
        );
    }
}
