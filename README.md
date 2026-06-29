yes > /dev/null.
 cargo test prove --release  -- --nocapture 

cargo test --release test_adaptive_trace -- --nocapture
cargo test --release --test bench_sort -- --nocapture


Per ogni fetta attiva:
  1. Istogramma locale
  2. Se nbuckets==1 (Uniform): copia diretta, fetta rimane in next_active
  3. Altrimenti: scatter + aggiungi sotto-fette con count>1 a next_active
  4. All'ultima passata: insertion sort sulle sotto-fette rimaste
  
## Strategie di Ordinamento: LSD vs MSD in SAPRI

Il motore di ordinamento SAPRI utilizza due strategie distinte per ottimizzare la performance in base alla natura dei dati e alle risorse hardware disponibili.

### 1. LSD (Least Significant Digit) - Radix Seriale
L'approccio **LSD (Radix Seriale)** ordina i dati partendo dalla cifra (o byte) meno significativa verso quella più significativa.

* **Comportamento:** È un approccio "brute-force" costante. Esegue sempre lo stesso numero di passate (es. 4 passate per un `u32`), 
	indipendentemente dai valori contenuti nell'array.
* **Vantaggi:**
    * **Predictability:** Il tempo di esecuzione è deterministico e costante.
    * **Hardware-Friendly:** Accede alla memoria in modo sequenziale. È altamente ottimizzato per il prefetcher hardware della CPU e satura la banda passante della RAM.
    * **Cache Locality:** Ideale per dataset di grandi dimensioni (> 5M elementi) dove la ricorsione causerebbe *cache thrashing*.
* **Quando usarlo:** È la scelta migliore per dataset casuali (`Full u32`), dati con alta entropia o quando è richiesta la massima stabilità nelle prestazioni.



### 2. MSD (Most Significant Digit) - Radix Adattivo
L'approccio **MSD (Adattivo)** ordina partendo dalla cifra più significativa, suddividendo ricorsivamente i dati in "bucket".

* **Comportamento:** È un approccio "intelligente" e chirurgico. Analizza la distribuzione dei dati e può terminare il lavoro prematuramente 
	se identifica che un sotto-blocco è già ordinato o composto da valori identici.
* **Vantaggi:**
    * **Early Exit:** Può "potare" rami dell'albero di ricerca, risparmiando cicli CPU preziosi.
    * **Efficienza su dati strutturati:** Eccelle con dati parzialmente ordinati, distribuzioni concentrate (es. ID utente con prefissi comuni) o cardinalità limitata.
    * **Performance:** Spesso supera l'LSD in scenari reali proprio perché evita passate non necessarie.
* **Quando usarlo:** Ideale per dati del mondo reale (log, timestamp, stringhe comuni), dove l'entropia non è massima.



---

### Tabella Comparativa: Quale scegliere?

| Scenario | Strategia Consigliata | Perché? |
| :--- | :--- | :--- |
| **Dati Casuali (Full Range)** | **LSD (Seriale)** | Massimizza il throughput della memoria; la logica adattiva paga solo overhead. |
| **Dataset Massicci (> 5M)** | **LSD (Seriale)** | Evita il rischio di frammentazione della cache causato dalla ricorsione. |
| **Dati Parzialmente Ordinati** | **MSD (Adattivo)** | La ricorsione rileva l'ordine e termina prima del previsto. |
| **Bassa Cardinalità (Duplicati)** | **MSD (Adattivo)** | Raggruppa rapidamente i valori identici in bucket, riducendo le passate. |
| **Performance Deterministiche** | **LSD (Seriale)** | Tempo di esecuzione garantito costante, ottimo per sistemi real-time. |

### Sintesi per l'implementazione
In SAPRI, la selezione della strategia è gestita dal *Dispatcher* (`execute_sapri_sort`). Se il parametro `use_adaptive` è impostato a `true`, 
il sistema bilancia dinamicamente MSD e LSD. Per dataset con alta entropia (test casuali), il sistema mantiene una stabilità del **~2.0x** rispetto al sort di sistema, 
dimostrando che l'integrazione tra logica adattiva e gestione ottimizzata della memoria è la chiave per prestazioni di livello database.

> **Nota tecnica:** Le prestazioni di SAPRI sono limitate dal Memory Bandwidth. L'ottimizzazione del 2.0x è stata raggiunta evitando 
l'allocazione dinamica dello scratch buffer durante il loop di esecuzione, mantenendo i dati 'caldi' nella cache L1/L2.
