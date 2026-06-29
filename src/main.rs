// src/main.rs

use sapri_sort::sapri_sort;
use sapri_sort::random_array::{DataFactory, GenStrategy};
use std::time::Instant;

fn main() {
    println!("==================================================");
    println!("          SAPRI ENGINE CLI - COGNE 64-BIT        ");
    println!("==================================================");

    // Configurazione rapida da CLI
    let size = 20_000_000; // 20 Milioni di elementi
    let strategy = GenStrategy::Grouped;
    let max_val = u32::MAX as u64; // Simulazione ottimizzazione 4 passate su RAM a 64-bit

    println!("Generazione di {} elementi a 64-bit ({:?})...", size, strategy);
    let master_data = DataFactory::generate_by_strategy(size, 0, max_val, &strategy);

    // --- TEST SAPRI ---
    let mut data_sapri = master_data.clone();
    println!("Esecuzione SAPRI Sort (Stabile)...");
    let start_sapri = Instant::now();
    sapri_sort(&mut data_sapri, max_val);
    let duration_sapri = start_sapri.elapsed();

    // --- TEST QUICKSORT ---
    let mut data_quick = master_data.clone();
    println!("Esecuzione Quicksort di sistema (Unstable)...");
    let start_quick = Instant::now();
    data_quick.sort_unstable();
    let duration_quick = start_quick.elapsed();

    // --- PRESTAZIONI ---
    println!("\n---------------- RISULTATI ----------------");
    println!("SAPRI:       {:?}", duration_sapri);
    println!("Quicksort:   {:?}", duration_quick);

    let sapri_ms = duration_sapri.as_secs_f64() * 1000.0;
    let quick_ms = duration_quick.as_secs_f64() * 1000.0;
    
    if sapri_ms < quick_ms {
        let speedup = quick_ms / sapri_ms;
        println!("==> SAPRI è {:.2}x più veloce di Quicksort!", speedup);
    } else {
        println!("==> Quicksort è più veloce del {:.2}%", ((sapri_ms - quick_ms) / quick_ms) * 100.0);
    }

    // --- VALIDAZIONE ---
    if data_sapri == data_quick {
        println!("[✓] Validazione Correttezza: OK (I dati corrispondono al 100%)");
    } else {
        println!("[!] ERRORE: SAPRI ha corrotto l'array!");
    }
    println!("==================================================");
}
