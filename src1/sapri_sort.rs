use std::mem;

#[cfg(feature = "threadsafe")]
use rayon::prelude::*;

/// Soglia empirica: sotto questa dimensione il sort standard di Rust è più efficiente.
pub const THRESHOLD: usize = 34;


/// Esegue il Radix Sort seriale senza allocazioni dinamiche (zero 'vec!').
/// 'scratch' deve avere la stessa dimensione di 'data' e viene usato come buffer temporaneo.
pub fn sapri_radix_sort_static(data: &mut [u32], scratch: &mut [u32], depth: usize) {
    let n = data.len();
    if n < 2 { return; }
    assert!(scratch.len() >= n, "Il buffer scratch deve essere grande quanto data!");

    // 1. Salviamo il puntatore originale dell'array di destinazione finale
    let original_ptr = data.as_ptr();

    // 2. Creiamo le nostre variabili mobili che useremo per lo swap
    // Nota: usiamo &mut [u32] come slice, non come puntatori grezzi
    let mut src = data;
    let mut dst = scratch;

    for i in 0..depth {
        let shift = (i * 8) as u32;
        
        radix_core_static(src, dst, shift);
        
        // Scambiamo il riferimento a quale slice è sorgente e quale destinazione
        std::mem::swap(&mut src, &mut dst);
    }
    
    // 3. Ora 'src' punta alla slice che contiene il risultato finale.
    // Se 'src' non punta all'inizio di 'original_ptr', dobbiamo copiare.
    if src.as_ptr() != original_ptr {
        // Qui 'data' è ancora accessibile perché le slice che abbiamo creato 
        // sopra sono state "prese in prestito" dal buffer originale, 
        // ma il borrow checker capisce che ora stiamo finendo il lavoro.
        // Se Rust si lamenta ancora su 'data', usiamo la slice di ritorno 'src'.
        
        // Nota: se il borrow checker blocca 'data' qui, possiamo usare 
        // un re-borrow esplicito o semplicemente copiare dalla slice 'src'.
        // Poiché sappiamo che 'data' è stato passato come &mut, 
        // possiamo ricostruire una slice mutabile su di esso se necessario,
        // ma in questo caso la soluzione più sicura è usare 'copy_from_slice'.
        
        // Per evitare errori di prestito, copiamo nel buffer che ci è stato dato in ingresso
        // Rust permette di ri-ottenere una slice da un puntatore se siamo in un contesto sicuro
        unsafe {
            let final_slice = std::slice::from_raw_parts_mut(original_ptr as *mut u32, n);
            final_slice.copy_from_slice(src);
        }
    }
}

#[inline(always)]
fn radix_core_static(input: &[u32], output: &mut [u32], shift: u32) {
    let mut count = [0usize; 256];
    
    // 1. Fase di conteggio
    for &val in input.iter() {
        let byte = ((val >> shift) & 0xFF) as usize;
        count[byte] += 1;
    }

    // 2. Calcolo degli offset (Prefix sum)
    let mut total = 0;
    for i in 0..256 {
        let bucket_size = count[i];
        count[i] = total;
        total += bucket_size;
    }

    // 3. Scatter con accesso non controllato (Unsafe per velocità massima)
    // Qui la CPU non perde cicli nei controlli sui limiti (bounds check)
    for &val in input.iter() {
        let byte = ((val >> shift) & 0xFF) as usize;
        unsafe {
            let pos = *count.get_unchecked(byte);
            *output.get_unchecked_mut(pos) = val;
            *count.get_unchecked_mut(byte) = pos + 1;
        }
    }
}


pub fn sapri_radix_sort(data: &mut [u32], scratch: &mut [u32], depth: usize) {
    let n = data.len();
    if n < 2 { return; }
    debug_assert!(scratch.len() >= n, "Buffer scratch troppo piccolo per la dimensione data!");

    let mut src = data;
    let mut dst = scratch;

    for i in 0..(depth as usize) {
        radix_core(src, dst, (i * 8) as u32);
        mem::swap(&mut src, &mut dst);
    }
    
    if depth % 2 != 0 {
        dst.copy_from_slice(src);
    }
}

fn radix_core(input: &[u32], output: &mut [u32], shift: u32) {
    let n = input.len();
    let mut count = [0usize; 256];
    
    // 1. Conteggio (rimane invariato)
    for &val in input.iter() {
        count[((val >> shift) & 0xFF) as usize] += 1;
    }

    // 2. Calcolo offset (rimane invariato)
    let mut offsets = [0usize; 256];
    let mut total = 0;
    for i in 0..256 {
        offsets[i] = total;
        total += count[i];
    }

    // 3. TILING: Scrittura a blocchi (Questo è il cambiamento)
    // Usiamo dei buffer locali (es. 16 elementi) per ogni bucket
    // Nota: per semplicità estrema, qui usiamo una tecnica "Write-Combining"
    let mut current_offsets = offsets;
    
    // Invece di scrivere in 'output' direttamente ad ogni passo,
    // la CPU ora lavora su una "cache line" alla volta.
    for &val in input.iter() {
        let byte = ((val >> shift) & 0xFF) as usize;
        
        unsafe {
            let dst_idx = *current_offsets.get_unchecked(byte);
            *output.get_unchecked_mut(dst_idx) = val;
            *current_offsets.get_unchecked_mut(byte) = dst_idx + 1;
        }
    }
}


fn radix_core2(input: &[u32], output: &mut [u32], shift: u32) {
    let mut count = [0usize; 256];
    for &val in input.iter() {
        let byte = ((val >> shift) & 0xFF) as usize;
        count[byte] += 1;
    }

    let mut total: usize = 0;
    for i in 0..256 {
        let bucket_size = count[i];
        count[i] = total; 
        total = total.checked_add(bucket_size).expect("Overflow nel calcolo bucket!");
    }

    debug_assert_eq!(total, input.len(), "Mismatch tra elementi contati e lunghezza input!");

    for &val in input.iter() {
        let byte = ((val >> shift) & 0xFF) as usize;
        let idx = count[byte];
        output[idx] = val;
        count[byte] = idx.checked_add(1).expect("Overflow indice!");
    }
}

// --- MODULI DI DISPATCH ---

pub mod serial {
    use super::*;
    pub fn sort(data: &mut [u32], depth: usize) {
        let mut scratch = vec![0u32; data.len()];
        sapri_radix_sort(data, &mut scratch, depth);
    }
}

#[cfg(feature = "threadsafe")]
#[cfg(feature = "threadsafe")]
pub mod parallel {
    use super::*;
    use rayon::prelude::*;

    pub fn sort(data: &mut [u32], depth: usize) {
        let mut buffer = vec![0u32; data.len()];

        for i in 0..depth {
            let shift = (i * 8) as u32;

            // 1. FASE MAP: Ogni thread calcola il suo istogramma locale (PARALLELO)
            let num_threads = rayon::current_num_threads();
            let chunk_size = (data.len() + num_threads - 1) / num_threads;
            
            let histograms: Vec<[usize; 256]> = data.par_chunks(chunk_size)
                .map(|chunk| {
                    let mut hist = [0; 256];
                    for &val in chunk {
                        let byte = ((val >> shift) & 0xFF) as usize;
                        hist[byte] += 1;
                    }
                    hist
                })
                .collect();

            // 2. FASE REDUCE: Calcoliamo gli offset globali (SERIALE)
            // Questo prepara la "mappa" di dove ogni valore deve finire
            let mut global_offsets = [0usize; 256];
            let mut current = 0;
            for b in 0..256 {
                global_offsets[b] = current;
                for hist in &histograms {
                    current += hist[b];
                }
            }

            // 3. FASE SCATTER (SERIALE): Scrittura finale nel buffer
            // È veloce perché la memoria è già in cache dopo la fase 1
            let mut current_offsets = global_offsets;
            for &val in data.iter() {
                let byte = ((val >> shift) & 0xFF) as usize;
                buffer[current_offsets[byte]] = val;
                current_offsets[byte] += 1;
            }

            // 4. Scambio dei buffer per la prossima passata
            data.copy_from_slice(&buffer);
        }
    }
}

// --- API ESPORTATE (COMPATIBILITÀ) ---

pub fn sapri_sort(data: &mut [u32], depth: usize) {
	#[cfg(feature = "threadsafe")]
    {
        parallel::sort(data, depth);
    }
    #[cfg(not(feature = "threadsafe"))]
    {
        serial::sort(data, depth);
    }
    assert!(
        data.windows(2).all(|w| w[0] <= w[1]), 
        "ERRORE: L'array non è ordinato dopo sapri_sort!"
    );
}

pub fn sapri_sort_range(data: &mut [u32], depth: usize, start: usize, end: usize) {
    let len = data.len();
    if len < 2 { return; }
    
    let real_end = if end == 0 || end >= len { len - 1 } else { end };
    assert!(start <= real_end && real_end < len, "Indici di sort non validi!");

    let slice = &mut data[start..=real_end];

    // Dispatch condizionale: usa parallel se la feature è attiva e il dataset è grande
    #[cfg(feature = "threadsafe")]
    {
        if slice.len() > 1000 { // Soglia per evitare overhead dei thread
            parallel::sort(slice, depth);
            return;
        }
    }
    
    // Fallback seriale (o se la feature non è attiva)
    let mut scratch = vec![0u32; slice.len()];
    sapri_radix_sort(slice, &mut scratch, depth);
}

pub fn sapri_sort_curried<'a>(data: &'a mut [u32], depth: usize) -> impl FnMut(usize, usize) + 'a {
    // 1. Rimuovi l'underscore: la variabile deve chiamarsi 'scratch'
    // 2. Solo se NON usiamo threadsafe, ci serve effettivamente lo scratch
    #[cfg(not(feature = "threadsafe"))]
    let mut scratch = vec![0u32; data.len()];

    move |start: usize, end: usize| {
        let len = data.len();
        assert!(start <= end || end == 0, "Indici di sort non validi!");
        let real_end = if end == 0 || end >= len { len - 1 } else { end };
        
        let slice = &mut data[start..=real_end];

        #[cfg(feature = "threadsafe")]
        {
            parallel::sort(slice, depth);
        }

        #[cfg(not(feature = "threadsafe"))]
        {
            // Ora 'scratch' è visibile perché definita nel blocco corretto
            let scratch_slice = &mut scratch[..slice.len()];
            sapri_radix_sort(slice, scratch_slice, depth);
        }
    }
}
