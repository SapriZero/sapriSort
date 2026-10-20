// tests/correctness.rs

use sapri_sort::{sapri_sort_max, sapri_sort_unstable_max, BitTopology};
use sapri_sort::init_parallel_engine;

#[path = "common/random_array.rs"]
mod random_array;
use random_array::{DataFactory, GenStrategy};

// ============================================================
// EDGE CASES & ATOMIC TESTS
// ============================================================

#[test]
fn test_empty_and_single_element_arrays() {
    init_parallel_engine(0);

    // 1. Test completely empty vector
    let mut empty_data: Vec<u64> = vec![];
	sapri_sort_unstable_max(&mut empty_data, u64::MAX, BitTopology::Adaptive);

    assert!(empty_data.is_empty(), "Empty array sorting failed");

    // 2. Test single element vector
    let mut single_data = vec![42u64];
    sapri_sort_unstable_max(&mut single_data, 100, BitTopology::Adaptive);
    assert_eq!(single_data, vec![42u64], "Single element array sorting failed");
}

#[test]
fn test_identical_elements_distribution() {
    init_parallel_engine(0);

    // Forces heavy collision handling since all values are identical
    let size = 5_000;
    let mut data = vec![123456u64; size];
    let reference = data.clone();

    sapri_sort_max(&mut data, 200_000);
    assert_eq!(data, reference, "Failed to correctly handle uniform identical arrays");
}

#[test]
fn test_already_sorted_and_reversed_arrays() {
    init_parallel_engine(0);

    // 1. Already sorted sequence
    let mut sorted_data: Vec<u64> = (0..10_000).collect();
    let ref_sorted = sorted_data.clone();
    sapri_sort_max(&mut sorted_data, 15_000);
    assert_eq!(sorted_data, ref_sorted, "Failed on already sorted input sequence");

    // 2. Reversed sequence (worst-case scenario for many traditional sort routines)
    let mut reversed_data: Vec<u64> = (0..10_000).rev().collect();
    sapri_sort_max(&mut reversed_data, 15_000);
    assert!(DataFactory::is_sorted(&reversed_data), "Failed to sort completely reversed data array");
}

// ============================================================
// INTEGRATION & STRATEGY STRESS TESTING
// ============================================================

#[test]
fn test_all_factory_strategies_correctness() {
    init_parallel_engine(0);

    let test_sizes = [15, 500, 50_000];
    let strategies = [
        GenStrategy::Grouped,
        GenStrategy::Concentrated,
        GenStrategy::Max90OnBlock,
    ];

    // Evaluate across combinations of ranges, sizes, and specific structural distributions
    for &size in &test_sizes {
        for &strategy in &strategies {
            // Test within 32-bit ceiling boundaries
            let max_val_32 = u32::MAX as u64;
            let mut data_32 = DataFactory::generate_by_strategy(size, 0, max_val_32, &strategy);
            let mut ref_32 = data_32.clone();
            ref_32.sort_unstable();

            sapri_sort_max(&mut data_32, max_val_32);
            assert_eq!(data_32, ref_32, "Validation failed for 32-bit size {} under strategy {:?}", size, strategy);

            // Test within native 64-bit boundaries
            let max_val_64 = (u32::MAX as u64) + 500_000_000;
            let mut data_64 = DataFactory::generate_by_strategy(size, 0, max_val_64, &strategy);
            let mut ref_64 = data_64.clone();
            ref_64.sort_unstable();

            sapri_sort_max(&mut data_64, max_val_64);
            assert_eq!(data_64, ref_64, "Validation failed for 64-bit size {} under strategy {:?}", size, strategy);
        }
    }
}
