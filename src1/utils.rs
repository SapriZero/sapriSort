use rand::{thread_rng, Rng};

pub fn eval_v<T>(condition: bool, val_true: T, val_false: T) -> T {
    if condition { val_true } else { val_false }
}

#[derive(Debug, Clone, Copy)]
pub enum GenStrategy {
    Grouped,
    Concentrated,
    Max90OnBlock,
}

pub struct DataFactory; // Non serve più conservare lo stato se usi thread_rng locale

impl DataFactory {
	pub fn generate_by_strategy(size: usize, min: u32, max: u32, strategy: &GenStrategy) -> Vec<u32> {
        match strategy {
            GenStrategy::Grouped => Self::generate_grouped(size, min, max),
            GenStrategy::Concentrated => Self::generate_concentrated(size, min, max),
            GenStrategy::Max90OnBlock => Self::generate_max90_on_block(size, max),
            // Aggiungi qui altre strategie se ne hai
        }
    }
    
    // --- Metodi di Generazione (Richiedono &mut self) ---
    pub fn generate_max90_on_block(size: usize, max: u32) -> Vec<u32> {
	    let mut rng = thread_rng();
	    const BLOCK_SIZE: u32 = 65536;
	    const MAX_VALUES: usize = 90;
	    const COMPRESSED_RATIO: f64 = 0.2;
	    
	    let num_blocks = eval_v(max == 0, 1, (max / BLOCK_SIZE + 1) as usize);
	    let mut block_values = vec![Vec::with_capacity(MAX_VALUES); num_blocks];
	    let mut values_per_block = vec![0; num_blocks];
	
	    let compressed_blocks = num_blocks / 10;
	    let compressed_size = (size as f64 * COMPRESSED_RATIO) as usize;
	
	    for i in 0..num_blocks {
	        let is_compressed = i < compressed_blocks;
	        
	        // Proteggiamo i range con eval_v
	        let count = if is_compressed {
	            eval_v(50 < MAX_VALUES, rng.gen_range(50..MAX_VALUES), 50)
	        } else {
	            eval_v(10 < MAX_VALUES, rng.gen_range(10..MAX_VALUES), 10)
	        };
	        
	        values_per_block[i] = count;
	        block_values[i] = (0..count)
	            .map(|_| {
	                let base = i as u32 * BLOCK_SIZE;
	                base + rng.gen_range(0..BLOCK_SIZE)
	            })
	            .collect();
	    }
	
	    (0..size).map(|i| {
	        // Protezione contro range vuoti (se compressed_blocks è 0)
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

    pub fn generate_grouped(size: usize, min: u32, max: u32) -> Vec<u32> {
        let mut rng = thread_rng(); // Inizializza qui
        (0..size).map(|i| {
            let group_start = (min as u64 + (i / 100) as u64 * 65536) as u32;
            let base = std::cmp::min(group_start, max);
            base.saturating_add(rng.gen::<u32>() % 65536).min(max)
        }).collect()
    }

    pub fn generate_concentrated(size: usize, min: u32, max: u32) -> Vec<u32> {
        let mut rng = thread_rng(); // Inizializza qui
        let concentration_bit: u32 = 12345;
        (0..size).map(|_| {
            if rng.gen_bool(0.5) {
                let val = (concentration_bit << 16) + (rng.gen::<u32>() % 65536);
                val.clamp(min, max)
            } else {
                rng.gen_range(min..=max)
            }
        }).collect()
    }    

    // --- Metodi di Utility (Statici) ---W

    pub fn is_sorted_idx(arr: &[u32], start: usize, end: usize) -> Result<(), (usize, u32, u32)> {
        if arr.is_empty() { return Ok(()); }
        let safe_end = std::cmp::min(end, arr.len().saturating_sub(1));
        arr[start..=safe_end].windows(2).enumerate().try_for_each(|(i, w)| {
            if w[0] > w[1] { Err((start + i + 1, w[0], w[1])) } else { Ok(()) }
        })
    }

    pub fn compare_arrays(arr1: &[u32], arr2: &[u32]) -> Result<(), (usize, u32, u32)> {
        if arr1.len() != arr2.len() { return Err((0, arr1.len() as u32, arr2.len() as u32)); }
        arr1.iter().zip(arr2.iter()).enumerate().try_for_each(|(i, (&a, &b))| {
            if a != b { Err((i, a, b)) } else { Ok(()) }
        })
    }

    pub fn print_array(arr: &[u32], limit: usize) {
        println!("{:?}", &arr[..std::cmp::min(arr.len(), limit)]);
    }
    
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_generation_methods() {
        let size = 100;
        let min = 0;
        let max = 1000000;

        // Verifica che ogni metodo produca la dimensione corretta
        let data1 = DataFactory::generate_max90_on_block(size, max as u32);
        let data2 = DataFactory::generate_grouped(size, min, max as u32);
        let data3 = DataFactory::generate_concentrated(size, min, max as u32);

        assert_eq!(data1.len(), size);
        assert_eq!(data2.len(), size);
        assert_eq!(data3.len(), size);
    }

    #[test]
    fn test_sorting_and_comparison_logic() {
        let size = 200;
        let data = DataFactory::generate_grouped(size, 0, 1000000);
        
        // Verifica uguaglianza
        let data_clone = data.clone();
        assert!(DataFactory::compare_arrays(&data, &data_clone).is_ok());

        // Verifica discrepanza
        let mut data_modified = data.clone();
        // Invertiamo due elementi vicini per forzare un errore di ordinamento
        data_modified.swap(50, 51);
        
        assert!(DataFactory::compare_arrays(&data, &data_modified).is_err());
        assert!(DataFactory::is_sorted_idx(&data_modified, 0, size - 1).is_err());
    }

    #[test]
    fn test_edge_cases() {
        // Array vuoto
        let empty: [u32; 0] = [];
        assert!(DataFactory::is_sorted_idx(&empty, 0, 0).is_ok());
        
        // Array singolo
        let single = [10];
        assert!(DataFactory::is_sorted_idx(&single, 0, 0).is_ok());
    }
}
