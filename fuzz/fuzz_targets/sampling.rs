#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../crates/hoimin-cli/src/plan/sampling.rs"]
mod sampling;

fuzz_target!(|input: (u16, u16, u64)| {
    let (population, count, seed) = input;
    let population = usize::from(population % 4097);
    let count = usize::from(count);
    let ids = sampling::sample_indices(population, count, seed);
    assert_eq!(ids.len(), population.min(count));
    let mut seen = vec![false; population];
    for &id in &ids {
        assert!(id < population);
        assert!(!seen[id]);
        seen[id] = true;
    }
    let full = sampling::sample_indices(population, population, seed);
    assert_eq!(ids, full[..ids.len()]);
});
