// tests/bench_sort.rs
// Benchmark comparativi SAPRI vs sort_unstable.

use std::time::Instant;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use sapri_sort::prove::{inner_execute_with_scratch, execute_sapri_sort};
use sapri_sort::utils::DataFactory;

// ============================================================
// Helper per generazione dati rapida
// ============================================================

fn hash_seq(size: usize, modulo: u64) -> Vec<u32> {
    (0..size).map(|i| {
        let mut h = DefaultHasher::new();
        i.hash(&mut h);
        (h.finish() % modulo) as u32
    }).collect()
}

// ============================================================
// Core Bench Functions (Corrette per il Borrow Checker)
// ============================================================

fn bench_direct(label: &str, mut data: Vec<u32>, depth: usize) {
    let mut reference = data.clone();
    
    // 1. Benchmark Rust Standard
    let t0 = Instant::now();
    reference.sort_unstable();
    let t_ref = t0.elapsed();

    // 2. Benchmark SAPRI (Chiamata diretta al motore)
    let t1 = Instant::now();
    let len = data.len(); // Estrazione lunghezza per evitare borrow conflict
    execute_sapri_sort(&mut data, depth, false, 0, len);
    let t_sapri = t1.elapsed();

    // assert_eq!(data, reference, "[{}] Output diverso da sort_unstable!", label);
    if data != reference {
		// Stampa solo i primi 10 elementi per diagnosticare
		println!("\n[ERRORE] Mismatch in {}:", label);
		println!("Data head: {:?}", &data[..10.min(data.len())]);
		println!("Ref  head: {:?}", &reference[..10.min(reference.len())]);
		panic!("Output non corrispondente!");
	}

    let ratio = t_ref.as_secs_f64() / t_sapri.as_secs_f64();
    let win = if ratio >= 1.0 { "✓" } else { "✗" };
    println!("[{}] SAPRI (depth={}): {:?} | Ref: {:?} | {:.2}x  {}", 
        label, depth, t_sapri, t_ref, ratio, win);
}

// ============================================================
// Test di calibrazione Depth (Livelli L0, L1, L2, L3)
// ============================================================

#[test]
fn bench_depth_1() { bench_direct("L0 (8-bit range)", hash_seq(10_000_000, 255), 1); }

#[test]
fn bench_depth_2() { bench_direct("L1 (16-bit range)", hash_seq(10_000_000, 65_535), 2); }

#[test]
fn bench_depth_3() { bench_direct("L2 (24-bit range)", hash_seq(10_000_000, 16_000_000), 3); }

#[test]
fn bench_depth_4() { bench_direct("L3 (32-bit range)", hash_seq(10_000_000, u32::MAX as u64), 4); }

// ============================================================
// Benchmarks Standard 10M
// ============================================================

#[test]
fn bench_l0_256vals_10m() { bench_direct("L0 counting 256v 10M", hash_seq(10_000_000, 256), 1); }

#[test]
fn bench_l1_65k_10m() { bench_direct("L1 radix2pass 65K 10M", hash_seq(10_000_000, 65_536), 2); }



// ============================================================
// Pattern Reali con DataFactory
// ============================================================

#[test]
fn bench_grouped_10m() { bench_direct("Grouped 10M", DataFactory::generate_grouped(10_000_000, 0, u32::MAX), 4); }

#[test]
fn bench_concentrated_10m() { bench_direct("Concentrated 10M", DataFactory::generate_concentrated(10_000_000, 0, u32::MAX), 4); }

#[test]
fn bench_max90_10m() { bench_direct("Max90OnBlock 10M", DataFactory::generate_max90_on_block(10_000_000, u32::MAX), 4); }

// ============================================================
// Analisi Risparmio Passate
// ============================================================

#[test]
fn bench_passes_efficiency() {
    let base = hash_seq(10_000_000, 100_000); 

    let mut d3 = base.clone();
    let len3 = d3.len();
    let t0 = Instant::now();
    execute_sapri_sort(&mut d3, 3, false, 0, len3);
    let t3 = t0.elapsed();

    let mut d4 = base.clone();
    let len4 = d4.len();
    let t1 = Instant::now();
    execute_sapri_sort(&mut d4, 4, false, 0, len4);
    let t4 = t1.elapsed();

    assert_eq!(d3, d4);
    let gain = (1.0 - t3.as_secs_f64() / t4.as_secs_f64()) * 100.0;
    println!("Risparmio skipping passata: 3-pass: {:?} | 4-pass: {:?} | Guadagno: {:+.1}%", t3, t4, gain);
}

#[test]
fn bench_full_u32_10m_old() { bench_direct("L3 Full old u32 10M", hash_seq(10_000_000, u32::MAX as u64), 4); }

#[test]
fn bench_full_u32_10m() {
    // 1. Generazione dati (fuori dal tempo di misurazione)
    let data = hash_seq(10_000_000, u32::MAX as u64);
    
    // 2. Allocazione buffer scratch (fuori dal tempo di misurazione)
    // Questo è il segreto per recuperare il gap prestazionale
    let mut scratch = vec![0u32; data.len()]; 
    
    // 3. Prepariamo i dati per il test
    let mut data_clone = data.clone();
    let mut reference = data.clone();
    
    // 4. Benchmark Rust Standard (pdqsort)
    let t0 = Instant::now();
    reference.sort_unstable();
    let t_ref = t0.elapsed();
    
    // 5. Benchmark SAPRI (misuriamo SOLO l'algoritmo)
    let t1 = Instant::now();
    let len = data_clone.len();

   // use_adaptive a false quindi  (LSD)
   // use_adaptive a ftrue MSD (Adattiva)
	// use_adaptive = true Questo abilita la logica MSD, che è più "intelligente" nel gestire range di dati concentrati o strutture particolari, ma introduce un po' di overhead per la gestione della ricorsione.
	let use_adaptive = true;
	
    // inner_execute_with_scratch(&mut data_clone, &mut scratch, 4, use_adaptive, 0, len);
     execute_sapri_sort(&mut data_clone, 4, use_adaptive, 0, len);
    let t_sapri = t1.elapsed();

    // 6. Validazione e Output
    assert_eq!(data_clone, reference, "[L3 Full u32 10M] Output errato!");

    let ratio = t_ref.as_secs_f64() / t_sapri.as_secs_f64();
    let win = if ratio >= 1.0 { "✓" } else { "✗" };
    
    println!("[L3 Full u32 10M] SAPRI (depth=4): {:?} | Ref: {:?} | {:.2}x  {}", 
        t_sapri, t_ref, ratio, win);
}
