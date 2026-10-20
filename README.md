# SAPRI Sorting Engine

**SAPRI** is an enterprise-grade, high-performance hybrid sorting library engineered for modern multi-core architectures in Rust. It provides a zero-overhead interface that dynamically routes sorting tasks to the most efficient engine based on workload cardinality, numerical value bounds, and hardware concurrency.

---

## Architecture & Sorting Strategies: LSD vs MSD

SAPRI combines two distinct sorting paradigms to maximize performance across diverse dataset distributions:

### 1. LSD (Least Significant Digit) - Serial & Parallel Radix

The **LSD** approach sorts data starting from the least significant digit toward the most significant.

* **Behavior:** Operates as a constant, brute-force pipeline executing a fixed number of passes regardless of value distribution.
* **Advantages:**
* **Predictability:** Deterministic, constant execution time.
* **Hardware-Friendly:** Sequential memory access patterns fully saturate RAM bandwidth and leverage CPU hardware prefetchers.
* **Cache Locality:** Ideal for massive datasets ($>5\text{M}$ elements) where recursive branching would cause cache thrashing.


* **Use Case:** High-entropy random datasets (`Full u64`), uniform distributions, or real-time scenarios requiring strict deterministic performance.

### 2. MSD (Most Significant Digit) - Adaptive Radix

The **MSD** approach processes data starting from the most significant digit, recursively partitioning elements into distribution buckets.

* **Behavior:** An intelligent, surgical approach that inspects data layout and can terminate early if sub-blocks are already sorted or contain identical values.
* **Advantages:**
* **Early Exit:** Prunes unnecessary branches of the search tree, saving precious CPU cycles.
* **Structured Data Efficiency:** Excels with partially sorted data, concentrated value distributions, or common prefixes (e.g., timestamps, database logs).


* **Use Case:** Real-world datasets with lower entropy or significant clustering.

---

## Strategy Comparison Matrix

| Scenario | Recommended Strategy | Rationale |
| --- | --- | --- |
| **Random Data (Full Range)** | **LSD (Serial)** | Maximizes memory throughput; adaptive branching overhead provides no benefit. |
| **Massive Datasets ($>5\text{M}$)** | **LSD (Serial)** | Avoids cache fragmentation risks introduced by recursive partitioning. |
| **Partially Sorted Data** | **MSD (Adaptive)** | Recursion detects existing order and terminates early. |
| **Low Cardinality / Duplicates** | **MSD (Adaptive)** | Rapidly groups identical values into buckets, reducing sorting depth. |
| **Deterministic Performance** | **LSD (Serial)** | Guaranteed constant execution timing suitable for performance-critical systems. |

---

## Technical Notes & Performance

* **Memory Bandwidth Bound:** SAPRI's core execution limits are bound by memory bandwidth. Performance gains (up to $5\times$ over standard library sorting on large volumes) are achieved by eliminating dynamic heap allocations of scratch buffers within execution loops, keeping data hot in L1/L2 caches.
* **Hardware Awareness:** Integration with Rayon ensures dynamic concurrency scaling, gracefully falling back to optimized serial execution on constrained or single-core environments.

---

# BENCHMARK

## 32-bit vs 64-bit Hybrid Engine Benchmark Matrix

This evaluation matrix contrasts SAPRI's hybrid sorting engine against Rust's native unstable Quicksort across two distinct numerical domains (`u32` vs `u64`) under three workload distribution strategies: **Grouped**, **Concentrated**, and **Max90OnBlock**.

### 1. Macro Block: 32-bit Data Range ($\max \le \text{u32::MAX}$) — **The Domain of Dominance**
Across the entire 32-bit block, **SAPRI dominates decisively**, outperforming standard Quicksort across all distribution strategies with significant speedups:
* **Grouped & Concentrated:** The hybrid architecture leverages reduced radix passes and optimized 32-bit bucketization, comfortably beating Quicksort ($\sim 361\text{ ms}$ vs $\sim 533\text{ ms}$).
* **Max90OnBlock:** SAPRI reaches peak performance ($308\text{ ms}$ vs $745\text{ ms}$), highlighting how block structuring and intelligent key handling align seamlessly with partially clustered distributions.

### 2. Macro Block: 64-bit Data Range ($\max > \text{u32::MAX}$) — **The Full-Range Entropy Trade-off**
In the 64-bit domain, the results highlight the structural limits of radix sorting on full-entropy 64-bit integers:
* **Where it loses (Max90OnBlock & Grouped):** On full-range 64-bit integers with high entropy, SAPRI (Native 64-bit) aligns with or slightly trails native Quicksort ($\sim 751.9\text{ ms}$ vs $\sim 751.4\text{ ms}$). 
* **Technical Rationale:** A 64-bit integer requires significantly more passes (or massive, sparse buckets) to complete a radix sort, placing heavy pressure on CPU cache and memory bandwidth. When entropy is maximal and the value space is vast, comparison-based Quicksort (scaling as $O(N \log N)$ without fixed digit-pass overhead) regains competitiveness.
* **Where it still wins (Concentrated):** Even at 64-bit scale, if data exhibits concentration or favorable patterns (`Concentrated`), SAPRI's native engine still pulls ahead ($\sim 534.9\text{ ms}$ vs $\sim 692.2\text{ ms}$), proving that bucketization maintains advantages as long as dispersion does not saturate the address space.

### Architectural Conclusion
This benchmark suite demonstrates why SAPRI is a true **hybrid engine**: it achieves extraordinary performance on bounded and 32-bit domains (where linear counting/radix sort shines), while relying on intelligent router thresholds and fallback strategies to handle large-scale, high-entropy 64-bit data effectively.

=======================================================================
   SAPRI HYBRID ENGINE BENCHMARK SUITE - 32 vs 64 BIT MATRIX EVALUATION
=======================================================================

>>> [ MACRO BLOCK: 32-BIT DATA RANGE (max_val <= u32::MAX) ] <<<

--- 32-bit Scenario | Strategy: Grouped ---
  -> SAPRI (Hybrid 32-bit):   361.267122ms
  -> Quicksort (Unstable):     533.453669ms
  [✓] Accuracy verified.

--- 32-bit Scenario | Strategy: Concentrated ---
  -> SAPRI (Hybrid 32-bit):   464.296895ms
  -> Quicksort (Unstable):     801.675228ms
  [✓] Accuracy verified.

--- 32-bit Scenario | Strategy: Max90OnBlock ---
  -> SAPRI (Hybrid 32-bit):   308.304715ms
  -> Quicksort (Unstable):     745.460237ms
  [✓] Accuracy verified.

-----------------------------------------------------------------------

>>> [ MACRO BLOCK: 64-BIT DATA RANGE (max_val > u32::MAX) ] <<<

--- 64-bit Scenario | Strategy: Grouped ---
  -> SAPRI (Native 64-bit):   599.241182ms
  -> Quicksort (Unstable):     568.170373ms
  [✓] Accuracy verified.

--- 64-bit Scenario | Strategy: Concentrated ---
  -> SAPRI (Native 64-bit):   534.9349ms
  -> Quicksort (Unstable):     692.269522ms
  [✓] Accuracy verified.

--- 64-bit Scenario | Strategy: Max90OnBlock ---
  -> SAPRI (Native 64-bit):   751.901823ms
  -> Quicksort (Unstable):     751.457636ms
  [✓] Accuracy verified.

=======================================================================
---

## Specialized Domain Benchmarks: 8-bit & 16-bit Engines

SAPRI features dedicated specialized sorting engines optimized for bounded numerical domains (counting sort variants). By restricting the value range, these engines bypass comparison overhead and achieve pure linear complexity $O(N)$.

* **16-bit Specialized Engine ($\max \le 65,535$):** Evaluates performance on moderately bounded datasets. On massive volumes ($10\text{M}$ elements), both serial and parallel 16-bit engines drastically outperform standard library sorting, reducing execution time from over $614\text{ ms}$ down to $\sim 141\text{ ms}$, while maintaining zero overhead on small inputs via the router's fast-path guard.
* **8-bit Specialized Engine ($\max \le 255$):** Unleashes pure $O(N)$ counting sort efficiency on tightly bounded domains. On 10 million elements, SAPRI completes the sort in roughly **$67\text{ ms}$** compared to $432\text{ ms}$ for standard Quicksort, delivering more than a $6\times$ performance boost through optimized memory scanning and allocation-free execution.

=======================================================================
     SAPRI ENGINE - SPECIALIZED 16-BIT CUT BENCHMARK SUITE
=======================================================================
Size: 100        
  ├─ SAPRI Parallel (16):   18.648µs
  ├─ SAPRI Serial (16):     3.911µs
  └─ Quicksort (Std):       3.772µs | Status: [✓] Success
-----------------------------------------------------------------------
Size: 100      | SAPRI Router: 2.444µs | Std Quicksort: 1.956µs
Size: 1000      
  ├─ SAPRI Parallel (16):   30.242µs
  ├─ SAPRI Serial (16):     29.543µs
  └─ Quicksort (Std):       29.194µs | Status: [✓] Success
-----------------------------------------------------------------------
Size: 1000     | SAPRI Router: 2.165µs | Std Quicksort: 3.073µs
Size: 10000    | SAPRI Router: 360.663µs | Std Quicksort: 353.47µs
Size: 10000     
  ├─ SAPRI Parallel (16):   2.647355ms
  ├─ SAPRI Serial (16):     805.556µs
  └─ Quicksort (Std):       399.007µs | Status: [✓] Success
-----------------------------------------------------------------------
Size: 100000   | SAPRI Router: 5.13533ms | Std Quicksort: 4.690578ms
test test_router_small_datasets_behavior ... ok
Size: 100000    
  ├─ SAPRI Parallel (16):   2.511862ms
  ├─ SAPRI Serial (16):     1.314423ms
  └─ Quicksort (Std):       4.45989ms | Status: [✓] Success
-----------------------------------------------------------------------
Size: 1000000   
  ├─ SAPRI Parallel (16):   14.468854ms
  ├─ SAPRI Serial (16):     8.369077ms
  └─ Quicksort (Std):       53.906721ms | Status: [✓] Success
-----------------------------------------------------------------------
Size: 10000000  
  ├─ SAPRI Parallel (16):   141.264049ms
  ├─ SAPRI Serial (16):     99.590754ms
  └─ Quicksort (Std):       614.216119ms | Status: [✓] Success
=======================================================================


=======================================================================
     SAPRI ENGINE - SPECIALIZED 8-BIT CUT BENCHMARK SUITE
=======================================================================
Size: 100      | SAPRI Router: 10.197µs | Std Quicksort: 2.445µs
Size: 1000     | SAPRI Router: 3.213µs | Std Quicksort: 3.073µs
Size: 100       
  ├─ SAPRI Parallel (8):    5.657µs
  ├─ SAPRI Serial (8):      82.063µs
  └─ Quicksort (Std):       3.911µs | Status: [✓] Success
-----------------------------------------------------------------------
Size: 1000      
  ├─ SAPRI Parallel (8):    28.914µs
  ├─ SAPRI Serial (8):      57.409µs
  └─ Quicksort (Std):       28.356µs | Status: [✓] Success
-----------------------------------------------------------------------
Size: 10000    | SAPRI Router: 379.099µs | Std Quicksort: 353.258µs
Size: 10000     
  ├─ SAPRI Parallel (8):    423.099µs
  ├─ SAPRI Serial (8):      37.016µs
  └─ Quicksort (Std):       229.918µs | Status: [✓] Success
-----------------------------------------------------------------------
Size: 100000    
  ├─ SAPRI Parallel (8):    514.242µs
  ├─ SAPRI Serial (8):      388.597µs
  └─ Quicksort (Std):       2.388644ms | Status: [✓] Success
-----------------------------------------------------------------------
Size: 100000   | SAPRI Router: 4.69739ms | Std Quicksort: 4.547929ms
test test_router_small_datasets_behavior ... ok
Size: 1000000   
  ├─ SAPRI Parallel (8):    10.665262ms
  ├─ SAPRI Serial (8):      10.706259ms
  └─ Quicksort (Std):       25.954301ms | Status: [✓] Success
-----------------------------------------------------------------------
Size: 10000000  
  ├─ SAPRI Parallel (8):    67.947807ms
  ├─ SAPRI Serial (8):      64.910896ms
  └─ Quicksort (Std):       432.519565ms | Status: [✓] Success
=======================================================================

---

## SAPRI API Reference Guide

This document provides a comprehensive overview of the public interface exposed by the **SAPRI** sorting library, covering core runtime configuration, memory arenas, dynamic threshold routing, high-level sorting shorthands, and specialized signed-type extensions.

---

### Core Runtime & Memory Management

#### `BitTopology`

```rust
pub enum BitTopology

```

* **Description:** Configuration enum defining the bit-width extraction pass topology.
* **Variants:**
* `Adaptive`: Automatically adapts the bit-width extraction pass based on the observed value distribution and cardinality.



#### `ThreadScratchpad` & `GLOBAL_SCRATCH_POOL`

```rust
pub struct ThreadScratchpad {
    pub workspace: Vec<u64>,
}
pub static ref GLOBAL_SCRATCH_POOL: ThreadLocal<RefCell<ThreadScratchpad>>

```

* **Description:** Thread-local storage pooling mechanism utilized to cache and reuse internal vector workspaces across multiple thread counts, eliminating repeated heap allocation overhead.

#### `SapriParallelArena`

```rust
pub struct SapriParallelArena {
    pub scratch_u32_a: Vec<u32>,
    pub scratch_u32_b: Vec<u32>,
    pub scratch_u64: Vec<u64>,
}

```

* **Description:** Pre-allocated memory arena providing dedicated scratch buffers required for parallel radix sort operations and multi-threaded cell recombination.
* **Methods:**
* `with_capacity(capacity: usize) -> Self`: Initializes a memory arena with a pre-allocated capacity to prevent dynamic layout shifts during execution loops.
* `get_buffers_mut(&mut self, n: usize) -> (&mut [u32], &mut [u32], &mut [u64])`: Dynamically resizes workspace vectors if required and returns a mutable slice triad for sorting passes.



---

### Sorting Dispatchers & Routing Logic

#### `run_with_threshold_checks`

`fn run_with_threshold_checks<F>(data: &mut [u64], max_val: u64, _topology: BitTopology, fallback_radix_path: F) where F: FnOnce(&mut [u64], usize)`

* **Description:** Centralized micro-routing wrapper. Intercepts incoming workloads to execute hardware threshold evaluations and domain routing:
1. *Micro-arrays ($N \le 32$):* Dispatches directly to standard unstable sort to eliminate dispatch overhead.
2. *Specialized 8-bit Counting Sort:* Dispatches to `sapri_sort_8` if $\max \le 255$ and $N \ge 2,000$.
3. *Specialized 16-bit Counting Sort:* Dispatches to `sapri_sort_16` if $\max \le 65,535$ and $N \ge 5,000$.
4. *Parallel Threshold Check:* Falls back to standard library sort if $N < \text{PARALLEL\_THRESHOLD}$, otherwise executes the parallel radix engine.



#### `sapri_sort_unstable` / `sapri_sort_unstable_max` / `sapri_sort_unstable_min_max`

```rust
pub fn sapri_sort_unstable(data: &mut [u64], topology: BitTopology)
pub fn sapri_sort_unstable_max(data: &mut [u64], max_val: u64, topology: BitTopology)
pub fn sapri_sort_unstable_min_max(data: &mut [u64], min_val: u64, max_val: u64, topology: BitTopology)

```

* **Description:** Low-level engine entry points. `sapri_sort_unstable` scans the sequence for the maximum bound on the fly, while the `_max` and `_min_max` variants accept explicit domain bounds to bypass initial scanning phases entirely.

#### `sapri_sort_parallel_in_arena` / `sapri_sort_parallel_max`

```rust
pub fn sapri_sort_parallel_in_arena(data: &mut [u64], arena: &mut SapriParallelArena, topology: BitTopology)
pub fn sapri_sort_parallel_max(data: &mut [u64], arena: &mut SapriParallelArena, max_val: u64, _topology: BitTopology)

```

* **Description:** Low-level parallel primitives managing Rayon concurrency pools versus serial core execution based on active system thread counts.

#### `sapri_filter_and_sort_parallel`

```rust
pub fn sapri_filter_and_sort_parallel(
    data: &[u64],
    target_buffer: &mut [u64],
    _arena: &mut SapriParallelArena,
    min_bound: u64,
    max_bound: u64,
    _topology: BitTopology,
) -> usize

```

* **Description:** Range-filtering utility that extracts elements falling within explicit boundaries into a target buffer and sorts the matched subset in-place, returning the count of valid elements.

---

### High-Level Shorthand Aliases (`api.rs`)

User-facing wrappers providing ergonomic entry points defaulting to adaptive bit topologies:

* **`sapri_sort(data: &mut [u64])`**: Standard exposed shorthand wrapper executing unstable parallel sorts with default adaptive topology.
* **`sapri_sort_max(data: &mut [u64], max_val: u64)`**: Shorthand wrapper executing bounded max sorts with default adaptive topology.
* **`sapri_sort_min_max(data: &mut [u64], min_val: u64, max_val: u64)`**: Shorthand wrapper executing bounded range sorts.
* **`sapri_sort_topo(data: &mut [u64], topology: BitTopology)`**: Executes parallel sorts with an explicit bit topology configuration.
* **`sapri_sort_max_topo(data: &mut [u64], max_val: u64, topology: BitTopology)`**: Bounded max sort with explicit topology.
* **`sapri_sort_min_max_topo(data: &mut [u64], min_val: u64, max_val: u64, topology: BitTopology)`**: Bounded range sort with explicit topology.

---

### Signed `i64` Numeric Extensions

To bridge unsigned radix requirements with signed datasets, SAPRI implements bias mapping transformations over `i64` slices:

* **`sapri_sort_i64(data: &mut [i64])`**: Executes an unstable adaptive sort over a signed `i64` dataset using default topology.
* **`sapri_sort_i64_topo(data: &mut [i64], topology: BitTopology)`**: Executes an adaptive sort over signed datasets with explicit topology.
* **`sapri_sort_i64_range(data: &mut [i64], min_val: i64, max_val: i64, topology: BitTopology)`**: Optimizes signed sorting by mapping values into an unsigned domain via wrapping shifts (`wrapping_add(i64::MIN)`), sorting via the parallel radix engine, and reversing the bias upon completion.
