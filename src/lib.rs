// src/lib.rs

pub mod reorder_buckets;
pub mod random_array;

mod reorder_buckets_parallel;
pub use reorder_buckets_parallel::{sapri_radix_core, inizializza_motore_parallelo};

pub fn sapri_sort(data: &mut [u64], max_val: u64) {
    sapri_radix_core(data, max_val);
}
