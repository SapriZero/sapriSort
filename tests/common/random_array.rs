// tests/common/random_array.rs

#![allow(dead_code)]

use rand::{thread_rng, Rng};

/// Evaluates a condition and returns either the true or false value inline.
#[inline(always)]
pub fn eval_v<T>(condition: bool, val_true: T, val_false: T) -> T {
    if condition { val_true } else { val_false }
}

/// Supported strategic data distributions for engine stress-testing.

#[derive(Debug, Clone, Copy)]
pub enum GenStrategy {
    Grouped,          // group in order 
    GroupedUnordered, // group in not order 
    Concentrated,
    Max90OnBlock,
    Random,
}

pub struct DataFactory;

impl DataFactory {
    /// Generates a full dataset according to a specific distribution strategy.
    pub fn generate_by_strategy(size: usize, min: u64, max: u64, strategy: &GenStrategy) -> Vec<u64> {
        match strategy {
            GenStrategy::Grouped => Self::generate_grouped(size, min, max),
            GenStrategy::Concentrated => Self::generate_concentrated(size, min, max),
            GenStrategy::Max90OnBlock => Self::generate_max90_on_block(size, max),
            GenStrategy::Random => Self::generate_pure_random(size, min, max),
            GenStrategy::GroupedUnordered => {
                // Genera i classici gruppi pre-ordinati
                let mut data = Self::generate_grouped(size, min, max);
                // Mescola l'array per rompere il pattern sequenziale
                use rand::seq::SliceRandom;
                let mut rng = rand::thread_rng();
                data.shuffle(&mut rng);
                data
            }
        }
    }
    
    /// Generates highly clustered structures where 90% of values occupy distinct localized blocks.
    pub fn generate_max90_on_block(size: usize, max: u64) -> Vec<u64> {
        let mut rng = thread_rng();
        const BLOCK_SIZE: u64 = 65536;
        const MAX_VALUES: usize = 90;
        const COMPRESSED_RATIO: f64 = 0.2;
        
        let num_blocks = eval_v(max == 0, 1, (max / BLOCK_SIZE + 1) as usize);
        let mut block_values = vec![Vec::with_capacity(MAX_VALUES); num_blocks];
        let mut values_per_block = vec![0; num_blocks];
    
        let compressed_blocks = num_blocks / 10;
        let compressed_size = (size as f64 * COMPRESSED_RATIO) as usize;
    
        for i in 0..num_blocks {
            let is_compressed = i < compressed_blocks;
            let count = if is_compressed {
                eval_v(50 < MAX_VALUES, rng.gen_range(50..MAX_VALUES), 50)
            } else {
                eval_v(10 < MAX_VALUES, rng.gen_range(10..MAX_VALUES), 10)
            };
            
            values_per_block[i] = count;
            block_values[i] = (0..count)
                .map(|_| {
                    let base = i as u64 * BLOCK_SIZE;
                    base + rng.gen_range(0..BLOCK_SIZE)
                })
                .collect();
        }
    
        (0..size).map(|i| {
            let block_idx = if i < compressed_size && compressed_blocks > 0 {
                rng.gen_range(0..compressed_blocks)
            } else {
                rng.gen_range(0..num_blocks)
            };
            
            let limit = values_per_block[block_idx];
            let idx = eval_v(limit > 0, rng.gen_range(0..limit), 0);
            block_values[block_idx][idx]
        }).collect()
    }

    /// Generates sequential values clustered into local group steps.
    pub fn generate_grouped(size: usize, min: u64, max: u64) -> Vec<u64> {
        let mut rng = thread_rng();
        (0..size).map(|i| {
            let group_start = min.saturating_add((i / 100) as u64 * 65536);
            let base = std::cmp::min(group_start, max);
            base.saturating_add(rng.gen::<u64>() % 65536).min(max)
        }).collect()
    }

    /// Generates datasets concentrated on dense recurring boundaries.
    pub fn generate_concentrated(size: usize, min: u64, max: u64) -> Vec<u64> {
        let mut rng = thread_rng();
        let concentration_bit: u64 = 12345;
        (0..size).map(|_| {
            if rng.gen_bool(0.5) {
                let val = (concentration_bit << 16) + (rng.gen::<u64>() % 65536);
                val.clamp(min, max)
            } else {
                rng.gen_range(min..=max)
            }
        }).collect()
    }

	/// Generates a dataset distributed uniformly across the entire [min, max] range.
	pub fn generate_pure_random(size: usize, min: u64, max: u64) -> Vec<u64> {
	    let mut rng = thread_rng();
	    
	    // Ottimizzazione: se min e max coprono l'intero spettro di u64, usiamo la generazione veloce nativa.
	    // Altrimenti usiamo gen_range per rispettare i limiti richiesti dal benchmark.
	    if min == u64::MIN && max == u64::MAX {
	        (0..size).map(|_| rng.gen::<u64>()).collect()
	    } else {
	        (0..size).map(|_| rng.gen_range(min..=max)).collect()
	    }
	}

    /// Basic oracle function checking if a given array slice is strictly sorted.
    pub fn is_sorted(arr: &[u64]) -> bool {
        if arr.is_empty() { return true; }
        arr.windows(2).all(|w| w[0] <= w[1])
    }
}
