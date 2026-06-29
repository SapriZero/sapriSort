// src/sapri_interface.rs
use crate::sapri_sort;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortStrategy { Auto, CountingU8, RadixSerial, RadixParallel, RadixAdaptive }

#[derive(Debug, Clone, Copy)]
pub struct SortOptions {
    pub strategy: SortStrategy,
    pub known_max: Option<u32>,
    pub force_passes: Option<usize>,
}

impl Default for SortOptions {
    fn default() -> Self {
        Self { strategy: SortStrategy::Auto, known_max: None, force_passes: None }
    }
}

/// Aggancio pulito per le metriche di progressione del vecchio bench.rs
pub fn sort_with_progress<F>(data: &mut Vec<u32>, _opts: SortOptions, mut on_progress: F)
where F: FnMut(usize) {
    sapri_sort(data, 4);
    on_progress(100);
}
