//! Portable `splitmix64_fisher_yates_v1`. Changing any step requires a new version.

/// Forward partial Fisher–Yates over saved-plan indices, with unbiased bounded draws.
pub(crate) fn sample_indices(population: usize, count: usize, seed: u64) -> Vec<usize> {
    let count = count.min(population);
    let mut indices: Vec<_> = (0..population).collect();
    let mut state = seed;
    for index in 0..count {
        let bound = u64::try_from(population - index).expect("population fits u64");
        let offset = usize::try_from(draw_below(bound, || next_word(&mut state)))
            .expect("draw is below a usize population");
        indices.swap(index, index + offset);
    }
    indices.truncate(count);
    indices
}

fn next_word(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut word = *state;
    word = (word ^ (word >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    word = (word ^ (word >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    word ^ (word >> 31)
}

fn draw_below(bound: u64, mut next: impl FnMut() -> u64) -> u64 {
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let word = next();
        if word >= threshold {
            return word % bound;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_low_words_before_remainder() {
        let mut words = [0, 1, 5, 16].into_iter(); // 2^64 mod 10 = 6
        assert_eq!(draw_below(10, || words.next().unwrap()), 6);
        assert!(words.next().is_none());
        assert_eq!(draw_below(u64::MAX, || u64::MAX), 0);
        assert_eq!(draw_below(1, || 0), 0);
    }

    proptest! {
        #[test]
        fn ordered_sample_is_a_unique_bounded_reproducible_prefix(
            population in 0usize..512, count in 0usize..1024, seed in any::<u64>(),
        ) {
            let actual = sample_indices(population, count, seed);
            prop_assert_eq!(actual.len(), count.min(population));
            let unique: std::collections::BTreeSet<_> = actual.iter().copied().collect();
            prop_assert_eq!(unique.len(), actual.len());
            prop_assert!(actual.iter().all(|&id| id < population));
            let full = sample_indices(population, population, seed);
            prop_assert_eq!(&actual, &full[..actual.len()]);
        }
    }
}

#[cfg(test)]
#[test]
#[ignore = "descriptive fixed-seed bias inspection, not a statistical CI gate"]
fn sample_fixed_seed_frequency_inspection() {
    let mut selected = [0_u64; 8];
    let mut positions = [[0_u64; 8]; 3];
    for seed in 0..65_536 {
        for (position, id) in sample_indices(8, 3, seed).into_iter().enumerate() {
            selected[id] += 1;
            positions[position][id] += 1;
        }
    }
    println!(
        "{}",
        serde_json::json!({"seeds":"0..65536", "population":8,"count":3,"selected":selected,"positions":positions})
    );
}
