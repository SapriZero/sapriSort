// tests/bench_sort.rs

use sapri_sort::sapri_sort;
use sapri_sort::random_array::{DataFactory, GenStrategy};
use std::time::Instant;

fn safe_check_correctness(sapri: &[u64], quick: &[u64]) -> Result<(), (usize, u64, u64)> {
    if sapri.len() != quick.len() {
        return Err((0, sapri.len() as u64, quick.len() as u64));
    }
    for i in 0..sapri.len() {
        if sapri[i] != quick[i] {
            return Err((i, sapri[i], quick[i])); // Ritorna l'indice del primo errore riscontrato
        }
    }
    Ok(())
}

#[test]
fn esegui_benchmark_completo_ibrido_32_e_64_bit() {
    let size = 10_000_000;
    let strategie = [
        GenStrategy::Grouped,
        GenStrategy::Concentrated,
        GenStrategy::Max90OnBlock,
    ];

    println!("\n=======================================================================");
    println!("   SAPRI HYBRID ENGINE BENCHMARK SUITE - VERIFICA MATRICE 32 vs 64 BIT ");
    println!("=======================================================================");

    // =========================================================================
    // BLOCCO 1: SCENARI A 32 BIT (Ottimizzazione RAM & Scratch Compatto)
    // =========================================================================
    println!("\n>>> [MACRO BLOCCO: RANGE DATI A 32-BIT (max_val <= u32::MAX)] <<<");
    
    // Forza Rayon ad attivare tutti i core logici disponibili della CPU (es. 8, 16 o 32 core)
    sapri_sort::inizializza_motore_parallelo(0); 
    
    let max_val_32 = u32::MAX as u64;

    for strategy in &strategie {
        println!("\n--- Scenario 32-bit | Strategia: {:?} ---", strategy);
        let master_data = DataFactory::generate_by_strategy(size, 0, max_val_32, strategy);

        let mut data_sapri = master_data.clone();
        let start_sapri = Instant::now();
        sapri_sort(&mut data_sapri, max_val_32);
        let duration_sapri = start_sapri.elapsed();
        println!("  -> SAPRI (Ibrido 32-bit):   {:?}", duration_sapri);

        let mut data_quick = master_data.clone();
        let start_quick = Instant::now();
        data_quick.sort_unstable();
        let duration_quick = start_quick.elapsed();
        println!("  -> Quicksort (Unstable):     {:?}", duration_quick);

        // Validazione Sicura senza Dump a schermo
        match safe_check_correctness(&data_sapri, &data_quick) {
            Ok(_) => println!("  [✓] Correttezza verificata."),
            Err((idx, s_val, q_val)) => {
                panic!("\n[!] ERRORE DI ORDINAMENTO nei 32-bit all'indice [{}]: SAPRI ha {}, atteso {} (Quicksort)", idx, s_val, q_val);
            }
        }
    }

    println!("\n-----------------------------------------------------------------------");

    // =========================================================================
    // BLOCCO 2: SCENARI A 64 BIT (Core Nativo & Scratch Esteso)
    // =========================================================================
    println!("\n>>> [MACRO BLOCCO: RANGE DATI A 64-BIT (max_val > u32::MAX)] <<<");
    // NUOVO CODICE (Sicuro, pienamente a 64-bit, non corrompe DataFactory)
	// NUOVO CODICE (Sicuro, pienamente a 64-bit, non corrompe DataFactory)
// Imposta max_val a un valore superiore a u32::MAX per attivare i 64-bit, 
// ma sufficientemente contenuto per non far impazzire la matematica del DataFactory
let max_val_64 = (u32::MAX as u64) + 100_000_000;

    for strategy in &strategie {
        println!("\n--- Scenario 64-bit | Strategia: {:?} ---", strategy);
        let master_data = DataFactory::generate_by_strategy(size, 0, max_val_64, strategy);

        let mut data_sapri = master_data.clone();
        let start_sapri = Instant::now();
        sapri_sort(&mut data_sapri, max_val_64);
        let duration_sapri = start_sapri.elapsed();
        println!("  -> SAPRI (Nativo 64-bit):   {:?}", duration_sapri);

        let mut data_quick = master_data.clone();
        let start_quick = Instant::now();
        data_quick.sort_unstable();
        let duration_quick = start_quick.elapsed();
        println!("  -> Quicksort (Unstable):     {:?}", duration_quick);

        // Validazione Sicura senza Dump a schermo
        match safe_check_correctness(&data_sapri, &data_quick) {
            Ok(_) => println!("  [✓] Correttezza verificata."),
            Err((idx, s_val, q_val)) => {
                panic!("\n[!] ERRORE DI ORDINAMENTO nei 64-bit all'indice [{}]: SAPRI ha {}, atteso {} (Quicksort)", idx, s_val, q_val);
            }
        }
    }

    println!("\n=======================================================================");
}



