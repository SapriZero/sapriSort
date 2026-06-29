// src/sort_helper.rs
use rayon::prelude::*;

pub const CHUNK_SIZE:      usize = 128 * 1024;
pub const TINY_INSERTION:  usize = 32;
pub const TINY_UNSTABLE:   usize = 128;

#[inline(always)]
pub fn map_depth(depth: usize) -> usize {
    match depth {
        0 | 1 => 1,
        2     => 2,
        _     => 4,
    }
}

#[inline]
pub fn insertion_sort(data: &mut [u32]) {
    for i in 1..data.len() {
        let key = data[i];
        let mut j = i;
        while j > 0 && data[j - 1] > key {
            data[j] = data[j - 1];
            j -= 1;
        }
        data[j] = key;
    }
}

pub fn msd_recursive(data: &mut [u32], scratch: &mut [u32], start: usize, count: usize, shift: i32) {
    if count <= TINY_INSERTION {
        insertion_sort(&mut data[start..start + count]);
        return;
    }
    if count <= TINY_UNSTABLE {
        data[start..start + count].sort_unstable();
        return;
    }
    if shift < 0 { return; }

    let s = shift as u32;
    let mut counts = [0usize; 256];
    for &val in &data[start..start + count] { 
        counts[((val >> s) & 0xFF) as usize] += 1; 
    }

    let mut offsets = [0usize; 256];
    let mut current = 0usize;
    for i in 0..256 { 
        offsets[i] = current; 
        current += counts[i]; 
    }

    let mut pos = offsets;
    for &val in &data[start..start + count] {
        let b = ((val >> s) & 0xFF) as usize;
        scratch[start + pos[b]] = val;
        pos[b] += 1;
    }
    data[start..start + count].copy_from_slice(&scratch[start..start + count]);

    for i in 0..256 {
        if counts[i] > 1 {
            msd_recursive(data, scratch, start + offsets[i], counts[i], shift - 8);
        }
    }
}

pub fn radix_serial_logic(data: &mut [u32], scratch: &mut [u32], passes: usize) {
    let mut src_is_data = true;
    for i in 0..passes {
        let shift = (i * 8) as u32;
        if src_is_data { radix_pass_serial(data, scratch, shift); }
        else           { radix_pass_serial(scratch, data, shift); }
        src_is_data = !src_is_data;
    }
    if !src_is_data { data.copy_from_slice(scratch); }
}

pub fn counting_sort_u8(data: &mut [u32]) {
    let mut count = [0u32; 256];
    for &val in data.iter() { count[(val & 0xFF) as usize] += 1; }
    let mut pos = 0usize;
    for (val, &cnt) in count.iter().enumerate() {
        for _ in 0..cnt { data[pos] = val as u32; pos += 1; }
    }
}

pub fn counting_sort_u16(data: &mut [u32]) {
    let mut count = [0u32; 65536];
    // Conteggio (Histogramming)
    for &val in data.iter() { count[(val & 0xFFFF) as usize] += 1; }
    
    // Scrittura diretta
    let mut pos = 0usize;
    for (val, &cnt) in count.iter().enumerate() {
        for _ in 0..cnt { 
            data[pos] = val as u32; 
            pos += 1; 
        }
    }
}

pub fn counting_sort_u16_by_byte(data: &mut [u32]) {
    let mut scratch = vec![0u32; data.len()]; // Alloca una volta sola, o passa come parametro esterno
    
    // Passata 1: Byte basso (0-7)
    let mut count = [0usize; 256];
    for &val in data.iter() { count[(val & 0xFF) as usize] += 1; }
    
    let mut pos = [0usize; 256];
    let mut sum = 0;
    for i in 0..256 { pos[i] = sum; sum += count[i]; }
    
    for &val in data.iter() {
        let b = (val & 0xFF) as usize;
        scratch[pos[b]] = val;
        pos[b] += 1;
    }

    // Passata 2: Byte alto (8-15) - Scrittura diretta in data
    let mut count = [0usize; 256];
    for &val in scratch.iter() { count[((val >> 8) & 0xFF) as usize] += 1; }
    
    let mut pos = [0usize; 256];
    let mut sum = 0;
    for i in 0..256 { pos[i] = sum; sum += count[i]; }
    
    for &val in scratch.iter() {
        let b = ((val >> 8) & 0xFF) as usize;
        data[pos[b]] = val;
        pos[b] += 1;
    }
}

pub fn counting_sort_by_byte(data: &mut [u32], shift: u32) {
    let mut count = [0usize; 256];
    let mut scratch = vec![0u32; data.len()];

    // 1. Istogramma: conta solo il byte in posizione 'shift'
    for &val in data.iter() {
        let byte = ((val >> shift) & 0xFF) as usize;
        count[byte] += 1;
    }

    // 2. Prefissi (Posizionamento)
    let mut pos = [0usize; 256];
    let mut sum = 0;
    for i in 0..256 {
        pos[i] = sum;
        sum += count[i];
    }

    // 3. Scrittura nello scratch (Mantenendo i dati originali)
    for &val in data.iter() {
        let byte = ((val >> shift) & 0xFF) as usize;
        scratch[pos[byte]] = val;
        pos[byte] += 1;
    }

    // 4. Copia finale
    data.copy_from_slice(&scratch);
}

pub fn counting_sort_by_byte2(data: &mut [u32], shift: u32) {
    let mut count = [0u32; 256];
    
    // 1. Istogramma (Conteggio dei byte)
    for &val in data.iter() {
        let byte = ((val >> shift) & 0xFF) as usize;
        count[byte] += 1;
    }
    
    // 2. Scrittura diretta (Riposizionamento basato sul conteggio)
    // Nota: Questa tecnica sovrascrive l'array originale basandosi 
    // sull'istogramma, proprio come facevi per l'u16.
    let mut pos = 0usize;
    for (byte_val, &cnt) in count.iter().enumerate() {
        if cnt > 0 {
            // Qui stiamo ricostruendo l'array in base al valore del byte.
            // Attenzione: questo metodo è un Counting Sort puro. 
            // Per un Radix Sort completo che mantiene l'ordine degli altri bit, 
            // avresti bisogno di un array di scratch.
            for _ in 0..cnt {
                // Questo passaggio richiede attenzione: 
                // con il counting sort puro sovrascrivi i valori originali.
                // Funziona solo se stai facendo una singola passata definitiva.
                data[pos] = (byte_val as u32) << shift; 
                pos += 1;
            }
        }
    }
}

pub fn execute_sapri_sort_24bit(data: &mut [u32], scratch: &mut [u32]) {
    // 3 passate (8 bit alla volta)
    for shift in (0..24).step_by(8) {
        let mut count = [0u32; 256];

        // 1. Istogramma (Conteggio delle occorrenze)
        for &val in data.iter() {
            let byte = ((val >> shift) & 0xFF) as usize;
            count[byte] += 1;
        }

        // 2. Prefisso (Calcolo delle posizioni di output)
        let mut prefix = [0usize; 256];
        let mut sum = 0usize;
        for i in 0..256 {
            prefix[i] = sum;
            sum += count[i] as usize;
        }

        // 3. Scrittura (Spostamento nel buffer scratch)
        for &val in data.iter() {
            let byte = ((val >> shift) & 0xFF) as usize;
            scratch[prefix[byte]] = val;
            prefix[byte] += 1;
        }

        // 4. Copia di ritorno (Swap logico)
        data.copy_from_slice(scratch);
    }
}

#[inline(always)]
fn radix_pass_serial(src: &[u32], dst: &mut [u32], shift: u32) {
    let mut count = [0usize; 256];
    for &val in src { count[((val >> shift) & 0xFF) as usize] += 1; }
    let mut total = 0usize;
    for c in count.iter_mut() { 
        let old = *c; *c = total; total += old; 
    }
    for &val in src {
        let b = ((val >> shift) & 0xFF) as usize;
        unsafe {
            *dst.get_unchecked_mut(count[b]) = val;
            count[b] += 1;
        }
    }
}

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

#[inline(always)]
pub fn needed_passes(max_val: u32) -> usize {
    if max_val == 0 { return 1; }
    let bits = 32 - max_val.leading_zeros() as usize;
    (bits + 7) / 8
}

#[inline]
pub fn sample_max(data: &[u32]) -> u32 {
    let step = (data.len() / 1024).max(1);
    data.iter().step_by(step).cloned().max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper per verificare se un array è ordinato
    fn is_sorted(data: &[u32]) -> bool {
        data.windows(2).all(|w| w[0] <= w[1])
    }

    #[test]
    fn test_counting_sort_u16_fast() {
        // Nota: funziona solo se i valori sono <= 65535
        let mut data = vec![65535, 1, 3000, 2, 0, 100];
        counting_sort_u16(&mut data);
        assert!(is_sorted(&data));
    }

    #[test]
    fn test_counting_sort_by_byte_safe() {
        // Test con valori sparsi su 32 bit
        let mut data = vec![0xFF00FF, 0x000001, 0x00AA00, 0x111111];
        // Ordiniamo in base al byte meno significativo (shift 0)
        counting_sort_by_byte(&mut data, 0);
        
        // Verifica che il byte 0 sia ordinato
        let bytes: Vec<u32> = data.iter().map(|&x| x & 0xFF).collect();
        assert!(is_sorted(&bytes));
    }

    #[test]
    fn test_execute_sapri_sort_24bit() {
        let mut data = vec![1234567, 98765, 456, 1, 0, 16777215];
        let mut scratch = vec![0u32; data.len()];
        execute_sapri_sort_24bit(&mut data, &mut scratch);
        assert!(is_sorted(&data));
    }

    #[test]
    fn test_radix_serial_logic() {
        let mut data = vec![500, 10, 3, 9999, 1, 0, 255];
        let mut scratch = vec![0u32; data.len()];
        // 2 passate coprono fino a 16-bit (0-65535)
        radix_serial_logic(&mut data, &mut scratch, 2);
        assert!(is_sorted(&data));
    }

    #[test]
    fn test_insertion_sort() {
        let mut data = vec![10, 5, 8, 1, 7];
        insertion_sort(&mut data);
        assert!(is_sorted(&data));
    }

    #[test]
    fn test_needed_passes() {
        assert_eq!(needed_passes(255), 1);    // 8 bit
        assert_eq!(needed_passes(256), 2);    // 9 bit
        assert_eq!(needed_passes(65535), 2);  // 16 bit
        assert_eq!(needed_passes(1000000), 3);// 20 bit
    }
}
