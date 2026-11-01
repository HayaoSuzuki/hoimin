#[path = "../../../../../../crates/hoimin-cli/src/plan/sampling.rs"]
mod sampling;

fn property(population: usize, count: usize, actual: &[usize], full: &[usize]) {
    assert_eq!(actual.len(), count.min(population));
    let unique: std::collections::BTreeSet<_> = actual.iter().copied().collect();
    assert_eq!(unique.len(), actual.len());
    assert!(actual.iter().all(|&id| id < population));
    assert_eq!(actual, &full[..actual.len()]);
}

fn check(input: (u16, u16, u64)) {
    let (population, count, seed) = input;
    let population = usize::from(population % 512);
    let count = usize::from(count % 1024);

    let actual = sampling::sample_indices(population, count, seed);
    let full = sampling::sample_indices(population, population, seed);

    property(population, count, &actual, &full);
}

fn main() {
    let mode = std::env::args().nth(1).expect("mode required");
    match mode.as_str() {
        "replay" => check((8, 3, 42)),
        "empty" => check((0, 0, 0)),
        "full" => check((511, 1023, u64::MAX)),
        "corrupt-count" => property(3, 2, &[0], &[0, 1, 2]),
        "corrupt-unique" => property(3, 2, &[0, 0], &[0, 0, 2]),
        "corrupt-range" => property(3, 2, &[0, 3], &[0, 3, 1]),
        "corrupt-prefix" => property(3, 2, &[0, 1], &[1, 0, 2]),
        "proptest" => {
            use proptest::prelude::*;
            let config = proptest::test_runner::Config {
                cases: 256,
                failure_persistence: None,
                rng_seed: proptest::test_runner::RngSeed::Fixed(1),
                ..Default::default()
            };
            proptest::test_runner::TestRunner::new(config)
                .run(&any::<(u16, u16, u64)>(), |input| {
                    check(input);
                    Ok(())
                })
                .unwrap();
        }
        "bolero" => {
            bolero::check!()
                .with_type::<(u16, u16, u64)>()
                .with_iterations(256)
                .for_each(|input| check(*input));
        }
        _ => panic!("unknown research mode"),
    }
}
