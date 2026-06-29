// src/sort_helper.rs

pub const CHUNK_SIZE:      usize = 128 * 1024;
pub const TINY_INSERTION:  usize = 32;
pub const TINY_UNSTABLE:   usize = 128;


pub fn reorder_inplace_u16_unsafe(data: &mut [u32], part_start: &mut [usize]) {
    let n = data.len();
    // Prepariamo il BitSet: 1 bit per ogni elemento dell'array.
    // Usiamo un Vec<u64> per rendere i controlli ancora più veloci su sistemi a 64 bit.
    let mut visited = vec![0u64; (n + 63) / 64]; 

    for i in 0..n {
        // Controllo veloce del bit "visited"
        if unsafe { (*visited.get_unchecked(i >> 6) & (1 << (i & 63))) != 0 } {
            continue;
        }

        let mut current_val = unsafe { *data.get_unchecked(i) };
        let mut pos = i;

        loop {
            // Estrazione 16-bit (L1 range)
            let bucket = (current_val & 0xFFFF) as usize;
            
            // Otteniamo la destinazione e aggiorniamo il puntatore del bucket
            let dest = unsafe {
                let d_ptr = part_start.get_unchecked_mut(bucket);
                let d = *d_ptr;
                *d_ptr += 1;
                d
            };

            // Segniamo la posizione di destinazione come visitata nel BitSet
            unsafe {
                *visited.get_unchecked_mut(dest >> 6) |= 1 << (dest & 63);
            }

            if dest == i {
                // Il ciclo si è chiuso: rimettiamo l'ultimo valore "in volo" nella cella iniziale
                unsafe { *data.get_unchecked_mut(i) = current_val; }
                break;
            }

            // Scambio a catena (Cycle-Following)
            // Leggiamo il valore che stiamo per sovrascrivere e piazziamo quello corrente
            let next_val = unsafe { *data.get_unchecked(dest) };
            unsafe { *data.get_unchecked_mut(dest) = current_val; }
            
            current_val = next_val;
            pos = dest;
        }
    }
}

pub fn reorder_inplace_u16_unsafe2(data: &mut [u32], part_start: &mut [usize]) {
    let n = data.len();
    // Prepariamo il BitSet: 1 bit per ogni elemento dell'array.
    // Usiamo un Vec<u64> per rendere i controlli ancora più veloci su sistemi a 64 bit.
    let mut visited = vec![0u64; (n + 63) / 64]; 

    for i in 0..n {
        // Controllo veloce del bit "visited"
        if unsafe { (*visited.get_unchecked(i >> 6) & (1 << (i & 63))) != 0 } {
            continue;
        }

        let mut current_val = unsafe { *data.get_unchecked(i) };
        let mut pos = i;

        loop {
            // Estrazione 16-bit (L1 range)
            let bucket = (current_val & 0xFFFF) as usize;
            
            // Otteniamo la destinazione e aggiorniamo il puntatore del bucket
            let dest = unsafe {
                let d_ptr = part_start.get_unchecked_mut(bucket);
                let d = *d_ptr;
                *d_ptr += 1;
                d
            };

            // Segniamo la posizione di destinazione come visitata nel BitSet
            unsafe {
                *visited.get_unchecked_mut(dest >> 6) |= 1 << (dest & 63);
            }

            if dest == i {
                // Il ciclo si è chiuso: rimettiamo l'ultimo valore "in volo" nella cella iniziale
                unsafe { *data.get_unchecked_mut(i) = current_val; }
                break;
            }

            // Scambio a catena (Cycle-Following)
            // Leggiamo il valore che stiamo per sovrascrivere e piazziamo quello corrente
            let next_val = unsafe { *data.get_unchecked(dest) };
            unsafe { *data.get_unchecked_mut(dest) = current_val; }
            
            current_val = next_val;
            pos = dest;
        }
    }
}
