use std::time::Instant;
use rand::Rng;

// Direct imports from the root of the public crate API
use sapri_sort::{sapri_sort_unstable, init_parallel_engine};

/// Represents the bit-depth configuration boundary defining the upper limits of the input data.
#[derive(Debug, Clone, Copy)]
enum BitWindow {
    /// Full 64-bit space constraint: `u64::MAX`.
    Bits64,
    /// Standard 32-bit ceiling constraint: `u32::MAX` (triggers the specialized 32-bit engine fallback).
    Bits32,
    /// 24-bit range space constraint: $2^{24} - 1$.
    Bits24,
    /// 16-bit range space constraint: `u16::MAX`.
    Bits16,
    /// 8-bit range space constraint: `u8::MAX`.
    Bits8, 
    /// Narrow 4-bit range space constraint: values from 0 up to 15.
    Bits4, 
}

#[test]
fn execute_sapri_bit_matrix_benchmark() {
    // Initializes the global parallel thread pool (0 = auto-detect physical core topology)
    init_parallel_engine(0);

    println!("\n=======================================================================");
    println!("        SAPRI ENGINE - BIT-WINDOW RANGE ADAPTIVITY BENCHMARK");
    println!("=======================================================================");

    // Fixed high-capacity test size (10 Million items) to accurately isolate bit-depth scaling factors
    let test_size = 10_000_000;

    // Ordered sequence of target bit windows to process sequentially
    let bit_windows = vec![
        BitWindow::Bits64,
        BitWindow::Bits32,
        BitWindow::Bits24,
        BitWindow::Bits16,
        BitWindow::Bits8,
        BitWindow::Bits4,
    ];

    println!("Executing micro-benchmark matrix on fixed size: {}\n", format_thousand_separator(test_size));

    for window in bit_windows {
        execute_bit_dependent_comparison(test_size, window);
    }
    
    println!("=======================================================================");
}

/// Generates dataset samples strictly bounded by the target bit window ceiling and logs run times.
fn execute_bit_dependent_comparison(size: usize, window: BitWindow) {
    let mut rng = rand::thread_rng();
    let mut data_sapri = vec![0u64; size];
    
    // Configure numerical upper boundaries and execute structured data synthesis
    let max_val: u64 = match window {
        BitWindow::Bits64 => {
            for i in 0..size { data_sapri[i] = rng.gen::<u64>(); }
            u64::MAX
        }
        BitWindow::Bits32 => {
            // Fill arrays across standard 32-bit boundaries
            for i in 0..size { data_sapri[i] = rng.gen::<u32>() as u64; }
            u32::MAX as u64
        }
        BitWindow::Bits24 => {
            // Keys up to 16,777,215 (evaluated across 3 radix sorting passes)
            let limit = (1u64 << 24) - 1;
            for i in 0..size { data_sapri[i] = rng.gen_range(0..=limit); }
            limit
        }
        BitWindow::Bits16 => {
            // Keys up to 65,535 (evaluated across 2 radix sorting passes)
            for i in 0..size { data_sapri[i] = rng.gen::<u16>() as u64; }
            u16::MAX as u64
        }
        BitWindow::Bits8 => {
            // Keys up to 255 (evaluated across 1 single radix sorting pass)
            for i in 0..size { data_sapri[i] = rng.gen::<u8>() as u64; }
            u8::MAX as u64
        }
        BitWindow::Bits4 => {
            // Keys clamped from 0 to 15 (1 single radix pass with minimal structural entropy)
            for i in 0..size { data_sapri[i] = rng.gen_range(0..=15); }
            15
        }
    };

    let mut data_quicksort = data_sapri.clone();

    // --- TIMING SAPRI ENGINE ---
    let start_sapri = Instant::now();
    sapri_sort_unstable(&mut data_sapri, max_val);
    let duration_sapri = start_sapri.elapsed();

    // --- TIMING STANDARD QUICKSORT ---
    let start_quick = Instant::now();
    data_quicksort.sort_unstable();
    let duration_quick = start_quick.elapsed();

    // --- DATA INTEGRITY VALIDATION ---
    let correct = data_sapri == data_quicksort;
    let status = if correct { "[✓] Success" } else { "[X] INTEGRITY CRITICAL ERROR" };
    
    // Calculate the active algorithm sorting distribution pass count dynamically
    let bits = if max_val == 0 { 1 } else { 64 - max_val.leading_zeros() as usize };
    let estimated_passes = (bits + 7) / 8;

    println!(
        "Window: {:<8} (Passes: {}) | SAPRI: {:<12?} | Quicksort: {:<12?} | {}",
        format!("{:?}", window),
        estimated_passes,
        duration_sapri,
        duration_quick,
        status
    );
}

/// Helper utility converting raw sizes to international comma-separated formatting notations.
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
