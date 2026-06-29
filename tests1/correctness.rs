// tests/correctness.rs

// 1. Importa le funzioni atomiche dal modulo sort_helper
use sapri_sort::sort_helper::{
    counting_sort_u8, counting_sort_by_byte, insertion_sort,
    radix_serial_logic, msd_recursive, counting_sort_u16
};

// 2. Importa le funzioni di alto livello dal modulo prove
// Assicurati che in src/prove.rs queste funzioni siano marcate come 'pub'
use sapri_sort::prove::{
    execute_sapri_sort,
    inner_execute_with_scratch,
    phase_1_serial,
    compute_scatter_map,
    SortOptions,
    SortStrategy
};

// --- Helper di diagnostica ---
fn assert_sorted(data: &[u32], label: &str) {
    for i in 0..data.len().saturating_sub(1) {
        assert!(
            data[i] <= data[i+1],
            "[{}] pos {}: {} > {}", label, i, data[i], data[i+1]
        );
    }
}

// ============================================================
// Test di Integrazione e Dispatcher (Prove)
// ============================================================

#[test]
fn test_dispatcher_integration() {
    let cases = vec![
        ("L0 (depth 1)", vec![255, 0, 128, 1, 254], 1),
        ("L1 (depth 2)", vec![65535, 0, 1000, 255, 65534], 2),
        ("Standard (depth 4)", vec![100000, 0, 50000, 99999, 1], 4),
    ];
    
    for (label, input, depth) in cases {
        let mut data = input.clone();
        let mut reference = input.clone();
        reference.sort_unstable();
        
        let len = data.len(); // <--- Salviamo la len prima!
        execute_sapri_sort(&mut data, depth, true, 0, len);
        
        assert_eq!(&data, &reference, "Dispatcher fallito su: {}", label);
    }
}

#[test]
fn test_inner_execute_with_scratch_logic() {
    let mut data = vec![500, 2, 1, 8, 9, 3];
    let len = data.len(); // <--- Salviamo la len prima!
    let mut scratch = vec![0u32; len];
    
    inner_execute_with_scratch(&mut data, &mut scratch, 4, false, 0, len);
    
    assert!(data.windows(2).all(|w| w[0] <= w[1]));
}

// ============================================================
// Test della Logica di Supporto (Prove)
// ============================================================

#[test]
fn test_phase_1_serial_counting() {
    let data = vec![0x00, 0xFF, 0x00, 0xAA];
    let hist = phase_1_serial(&data, 0);
    assert_eq!(hist[0x00], 2);
    assert_eq!(hist[0xFF], 1);
    assert_eq!(hist[0xAA], 1);
}

#[test]
fn test_scatter_map_calculation() {
    // Definiamo correttamente i tipi a 256
    let hists = vec![[0u32; 256], [0u32; 256]]; 
    // Ora il tipo è Vec<[u32; 256]>, che convertito in slice è &[[u32; 256]]
    let smap = compute_scatter_map(&hists);
    
    assert_eq!(smap.global_offsets[0], 0);
}
