// src/shuffle.rs
//
// Steel — shuffle utilities
//
// Purpose:
// - Provide deterministic and non-deterministic shuffling helpers.
// - Avoid pulling in `rand` for small use-cases (planning randomization, test fuzz-ish order,
//   load-balancing, stable "random" ordering with a seed).
// - Offer:
//   - Fisher–Yates shuffle for slices
//   - deterministic shuffle with a seed (SplitMix64)
//   - stable permutation from a seed without modifying input (index mapping)
//   - choose N distinct items (sample without replacement)
//
// Notes:
// - Not cryptographically secure.
// - Deterministic mode is intended for reproducible builds/tests.
// - For cryptographic needs, use OS RNG / dedicated crate.
//
// API overview:
// - shuffle_in_place(slice, &mut rng)
// - shuffle_in_place_seeded(slice, seed)
// - shuffled_indices(len, seed) -> Vec<usize>
// - sample_indices(len, n, seed) -> Vec<usize>

#![allow(dead_code)]

use std::fmt;

/* ============================== RNG: SplitMix64 ============================== */

/// Small fast RNG with good statistical properties for non-crypto use.
/// Deterministic across platforms.
#[derive(Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        // SplitMix64 reference
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    #[inline]
    pub fn next_usize(&mut self) -> usize {
        // Reduce modulo via 64-bit; fine for our use.
        self.next_u64() as usize
    }

    /// Uniform integer in [0, bound), using rejection sampling to remove modulo bias.
    pub fn gen_range(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            return 0;
        }
        let b = bound as u64;

        // rejection threshold: largest multiple of b that fits u64
        let zone = u64::MAX - (u64::MAX % b);

        loop {
            let x = self.next_u64();
            if x < zone {
                return (x % b) as usize;
            }
        }
    }
}

impl fmt::Debug for SplitMix64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SplitMix64")
            .field("state", &format_args!("0x{:016x}", self.state))
            .finish()
    }
}

/* ============================== shuffle ============================== */

/// Fisher–Yates shuffle in place using provided RNG.
pub fn shuffle_in_place<T>(slice: &mut [T], rng: &mut SplitMix64) {
    let n = slice.len();
    if n <= 1 {
        return;
    }
    // iterate from end to start
    for i in (1..n).rev() {
        let j = rng.gen_range(i + 1);
        slice.swap(i, j);
    }
}

/// Deterministic shuffle in place from a seed.
pub fn shuffle_in_place_seeded<T>(slice: &mut [T], seed: u64) {
    let mut rng = SplitMix64::new(seed);
    shuffle_in_place(slice, &mut rng);
}

/// Return a deterministic permutation of indices [0..len).
pub fn shuffled_indices(len: usize, seed: u64) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..len).collect();
    shuffle_in_place_seeded(&mut idx, seed);
    idx
}

/// Sample `n` distinct indices from [0..len) without replacement (deterministic).
/// - If n >= len, returns full shuffled indices.
pub fn sample_indices(len: usize, n: usize, seed: u64) -> Vec<usize> {
    if n >= len {
        return shuffled_indices(len, seed);
    }

    // Use partial Fisher–Yates on indices (O(len) memory, O(n) swaps).
    let mut idx: Vec<usize> = (0..len).collect();
    let mut rng = SplitMix64::new(seed);

    for i in 0..n {
        let j = i + rng.gen_range(len - i);
        idx.swap(i, j);
    }
    idx.truncate(n);
    idx
}

/// Sample `n` distinct items (clones) from slice without replacement (deterministic).
pub fn sample_items<T: Clone>(slice: &[T], n: usize, seed: u64) -> Vec<T> {
    let idx = sample_indices(slice.len(), n, seed);
    idx.into_iter().map(|i| slice[i].clone()).collect()
}

/* ============================== convenience ============================== */

/// Shuffle by hashing each element index with seed and sorting by the hash.
/// - Stable and deterministic.
/// - O(n log n), but doesn't need swaps and is stable for "virtual shuffles".
pub fn shuffled_indices_by_hash(len: usize, seed: u64) -> Vec<usize> {
    let mut v: Vec<(u64, usize)> = (0..len)
        .map(|i| (mix64(seed ^ (i as u64).wrapping_mul(0x9E3779B97F4A7C15)), i))
        .collect();
    v.sort_by_key(|(h, _)| *h);
    v.into_iter().map(|(_, i)| i).collect()
}

#[inline]
fn mix64(mut x: u64) -> u64 {
    // A strong 64-bit mixer (similar to SplitMix finalizer)
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 31;
    x
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffle_seeded_is_deterministic() {
        let mut a = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut b = a.clone();

        shuffle_in_place_seeded(&mut a, 123);
        shuffle_in_place_seeded(&mut b, 123);

        assert_eq!(a, b);
    }

    #[test]
    fn shuffled_indices_len() {
        let idx = shuffled_indices(10, 1);
        assert_eq!(idx.len(), 10);

        let mut sorted = idx.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn sample_indices_distinct() {
        let s = sample_indices(100, 10, 999);
        assert_eq!(s.len(), 10);

        let mut sorted = s.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 10);
    }

    #[test]
    fn sample_indices_full_when_n_ge_len() {
        let s = sample_indices(5, 99, 1);
        assert_eq!(s.len(), 5);

        let mut sorted = s.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn shuffled_indices_by_hash_is_permutation() {
        let idx = shuffled_indices_by_hash(50, 42);
        assert_eq!(idx.len(), 50);

        let mut sorted = idx.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn rng_range_bounds() {
        let mut rng = SplitMix64::new(1);
        for _ in 0..10_000 {
            let x = rng.gen_range(7);
            assert!(x < 7);
        }
    }
}
