# SAPRI

[![Crates.io](https://img.shields.io/crates/v/sapri.svg)](https://crates.io/crates/sapri)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

SAPRI is a Rust library for sorting large slices of integers whose values fit in **8, 16 or 32 bits**. It chooses between a counting sort and a radix sort depending on the largest value in the data and on the number of elements. Outside that range it does not claim to be faster than the standard library.

It is not a new algorithm. Counting sort and LSD radix sort are well known techniques; what the library adds is the selection between them, tuned thresholds, and some implementation choices described below.

---

## When to use it

The input type is always `&mut [u64]`. The engine is chosen from the **maximum value** found in the data (or passed explicitly), not from the element type.

| Largest value | Engine | Use it when | Do not expect a gain when |
| --- | --- | --- | --- |
| up to 255 (8 bit) | Counting sort, serial | 100,000 elements or more | very small lists |
| up to 65,535 (16 bit) | Counting sort, serial | 100,000 elements or more | fewer than ~100,000 elements (see note) |
| up to 4,294,967,295 (32 bit) | LSD radix sort, parallel | 2,000,000 elements or more | fewer than 2,000,000 elements |
| above that (64 bit) | LSD radix sort, parallel | not recommended | always: no measurable advantage |

Notes:

* **Small lists.** Below the thresholds the library calls the standard `sort_unstable`, so you get standard performance, not a gain. The 32-bit threshold is `PARALLEL_THRESHOLD = 2,000,000`.
* **16 bit.** The counting table has 65,536 counters, a fixed cost that has to be spread over enough elements. At 10,000 elements the 16-bit engine was about twice as slow as `sort_unstable` (806 µs vs 399 µs). The break-even point lies between 10,000 and 100,000 elements and has not been measured more finely.
* **Parallel 8 and 16 bit.** Parallel variants exist but are not faster than the serial ones in our measurements. Use the serial engines for these ranges.
* **Data layout.** Only plain integers are sorted, with no attached payload.

---

## How it works

* **8 and 16 bit: counting sort.** One pass counts how many times each value occurs, a second pass rewrites the slice from those counts. No comparisons are made, so the cost is linear in the number of elements plus the size of the table.
* **32 bit: LSD radix sort.** Four passes of 8 bits, from the least significant byte. Intermediate buffers use `u32` instead of `u64`, which halves the memory traffic of the middle passes. A pass is skipped when all elements share the same byte at that position. In the parallel engine each block keeps its own histogram and write offsets, so the scatter step is stable and needs no locking.
* **Why 64 bit does not pay.** A 64-bit radix sort needs eight passes over 8-byte elements. The work is limited by memory bandwidth, and a comparison sort that works mostly in cache catches up when the values are spread over the whole range.

---

## Measurements

All times compare SAPRI with `slice::sort_unstable` from the standard library (single-threaded). Higher "gain" means SAPRI is faster.

**Test conditions:** `[fill in: CPU model, cores/threads, RAM, OS, rustc version, release profile, number of runs and whether median or single run]`

### 8 bit (values up to 255), serial engine

| Elements | SAPRI | `sort_unstable` | Gain |
| --- | --- | --- | --- |
| 100,000 | 389 µs | 2.39 ms | ~6.1x |
| 1,000,000 | 10.7 ms | 26.0 ms | ~2.4x |
| 10,000,000 | 64.9 ms | 432.5 ms | ~6.7x |

### 16 bit (values up to 65,535), serial engine

| Elements | SAPRI | `sort_unstable` | Gain |
| --- | --- | --- | --- |
| 10,000 | 806 µs | 399 µs | slower (~0.5x) |
| 100,000 | 1.31 ms | 4.46 ms | ~3.4x |
| 1,000,000 | 8.37 ms | 53.9 ms | ~6.4x |
| 10,000,000 | 99.6 ms | 614.2 ms | ~6.2x |

### 32 bit (values up to u32::MAX), parallel engine

Number of elements: `[fill in: N used in the suite]`. Threads: `[fill in]`.

| Distribution | SAPRI | `sort_unstable` | Gain |
| --- | --- | --- | --- |
| Grouped | 361 ms | 533 ms | ~1.5x |
| Concentrated | 464 ms | 802 ms | ~1.7x |
| Max90OnBlock | 308 ms | 745 ms | ~2.4x |

`[fill in: one line describing each distribution]`

---

## 64 bit

Functions for values above `u32::MAX` are included, and the results are verified as correct, but they do not give a measurable advantage: on data spread over the full range the times are equal to or worse than `sort_unstable`. These functions are kept for completeness and are not part of the supported use cases. For 64-bit data, use the standard library.

---

## Known limits

* Parallel scaling with the number of cores has not been measured. Do not assume it scales linearly: the sort is bound by memory bandwidth, and on few cores the gain over the serial path may be small.
* The comparison is against the single-threaded standard sort, not against a parallel comparison sort such as Rayon's `par_sort_unstable`.
* Signed `i64` helpers are provided (values are shifted into the unsigned domain and back). They are not benchmarked here.

---

## Installation

```toml
[dependencies]
sapri = "0.1"
```

Main entry points: `sapri_sort` (maximum found automatically), `sapri_sort_max` (maximum given by the caller, skips the scan), `sapri_sort_min_max` (range given by the caller). `init_parallel_engine` sets the number of worker threads.

---

## License

This project is licensed under the [MIT License](LICENSE).

[![Support on Ko-fi](https://img.shields.io/badge/Ko--fi-Donate%20a%20Coffee-ff5e5b?style=for-the-badge&logo=ko-fi&logoColor=white)](https://ko-fi.com/saprizero)
