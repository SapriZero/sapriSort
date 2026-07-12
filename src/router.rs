// src/router.rs

/// Struttura di telemetria atomica (protetta da Mutex per supportare Rayon)
/// per tracciare le chiamate sui piccoli bucket durante il testing.
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
static BUCKET_LOGS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// Opzioni di configurazione per la rifinitura dei bucket intermedi.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmallBucketStrategy {
    /// Quicksort instabile nativo di Rust
    Quicksort,
    /// Counting Sort Rigenerativo senza Prefix Sum (8-bit)
    Counting8,
    /// Counting Sort Rigenerativo senza Prefix Sum (16-bit)
    Counting16,
}

// Configurazione globale di default (può essere modificata per i test)
const ACTIVE_STRATEGY: SmallBucketStrategy = SmallBucketStrategy::Counting8;
const MAX_LOG_BUCKETS: usize = 40;

/// Il nuovo punto di ingresso decisionale che sostituisce la vecchia rete algebrica.
pub fn router(data: &mut [u64]) {
    let n = data.len();
    if n <= 1 { return; }

    // 1. Log dei primi 40 bucket incontrati (Thread-Safe per Rayon)
    let current_call = CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    if current_call < MAX_LOG_BUCKETS {
        if let Ok(mut logs) = BUCKET_LOGS.lock() {
            logs.push((current_call, n));
        }
    }

    // 2. Routing decisionale in base alla taglia del bucket
    if n <= 32 {
        // Sotto i 32 elementi il Quicksort in-place (o la vecchia rete) è imbattibile
        data.sort_unstable();
    } else if n > 32 && n <= 2000 {
        // Lo SWEET SPOT per la rigenerazione diretta senza Prefix Sum
        match ACTIVE_STRATEGY {
            SmallBucketStrategy::Quicksort => data.sort_unstable(),
            SmallBucketStrategy::Counting8 => {
                // Estraggo la base del bucket (i bit alti comuni) prendendo il primo elemento
                let base_mask = data[0] & 0xFFFFFFFFFFFFFF00;
                counting_sort_u64_regenerative_8(data, base_mask);
            }
            SmallBucketStrategy::Counting16 => {
                let base_mask = data[0] & 0xFFFFFFFFFFFF0000;
                counting_sort_u64_regenerative_16(data, base_mask);
            }
        }
    } else {
        // Sopra i 2000 elementi delega al Quicksort per evitare l'overhead delle tabelle grandi
        data.sort_unstable();
    }
}

/// Counting Sort Rigenerativo a 8-bit adattato per interi a 64-bit condivisori di base.
fn counting_sort_u64_regenerative_8(data: &mut [u64], base_mask: u64) {
    let mut count = [0u32; 256];
    
    // Conteggio locale (Cache L1 garantita)
    for &val in data.iter() {
        count[(val & 0xFF) as usize] += 1;
    }
    
    // Scrittura sequenziale lineare (Zero Prefix Sum)
    let mut pos = 0usize;
    for (val, &cnt) in count.iter().enumerate() {
        let reconstructed_val = base_mask | (val as u64);
        for _ in 0..cnt { 
            data[pos] = reconstructed_val; 
            pos += 1; 
        }
    }
}

/// Counting Sort Rigenerativo a 16-bit adattato per interi a 64-bit condivisori di base.
fn counting_sort_u64_regenerative_16(data: &mut [u64], base_mask: u64) {
    let mut count = [0u32; 65536];
    
    // Conteggio locale (Impegna circa 256KB, monitora l'impatto su TravelMate)
    for &val in data.iter() {
        count[(val & 0xFFFF) as usize] += 1;
    }
    
    // Scrittura sequenziale lineare (Zero Prefix Sum)
    let mut pos = 0usize;
    for (val, &cnt) in count.iter().enumerate() {
        let reconstructed_val = base_mask | (val as u64);
        for _ in 0..cnt { 
            data[pos] = reconstructed_val; 
            pos += 1; 
        }
    }
}

/// Funzione pubblica per stampare a fine benchmark lo stato dei primi 40 bucket analizzati.
pub fn print_bucket_telemetry() {
    if let Ok(logs) = BUCKET_LOGS.lock() {
        println!("=== SAPRI TELEMETRIA PICCOLI BUCKET (Primi {}) ===", logs.len());
        for (id, size) in logs.iter() {
            println!("Bucket Leaf #{:02} -> Dimensione: {} elementi", id, size);
        }
        println!("==================================================");
    }
}
