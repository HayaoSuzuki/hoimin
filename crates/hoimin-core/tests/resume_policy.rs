use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::num::NonZeroUsize;
use std::time::Duration;

use hoimin_core::{
    CommandArg, FingerprintInput, FingerprintInputFile, LineRange, MutantTimeout, MutationProfile,
    MutationStatus, OutputConfig, OutputFormat, RawRunConfig, RawRunLimits, ResourceMode,
    ResumeDecision, RunConfig, SourceHash, StoredResult, TargetSlice, fingerprint, resume_policy,
};
use proptest::prelude::*;

#[test]
fn metrics_output_does_not_change_fingerprint() {
    let first_config = config_with_metrics("first.json");
    let second_config = config_with_metrics("second.json");
    let runtime = fixture_input();
    let first = FingerprintInput::from_config(
        &first_config,
        runtime.sources.clone(),
        runtime.targets.clone(),
        runtime.resource_mode,
    );
    let second = FingerprintInput::from_config(
        &second_config,
        runtime.sources,
        runtime.targets,
        runtime.resource_mode,
    );

    assert_ne!(first_config.output, second_config.output);
    assert_eq!(
        fingerprint(&first),
        fingerprint(&second),
        "metrics output is not execution compatibility"
    );
}

fn config_with_metrics(path: &str) -> RunConfig {
    RunConfig::try_from(RawRunConfig {
        files: vec!["src/a.py".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        output: OutputConfig {
            format: OutputFormat::Json,
            metrics: Some(path.into()),
        },
        ..RawRunConfig::default()
    })
    .unwrap()
}

#[test]
fn resume_decides_one_stored_result_without_collecting_the_run() {
    for status in [MutationStatus::Killed, MutationStatus::Survived] {
        assert_eq!(
            resume_policy(Some(&stored("m", status))),
            ResumeDecision::Reuse
        );
    }
    for status in [
        MutationStatus::Timeout,
        MutationStatus::OutOfMemory,
        MutationStatus::ProcessLimit,
        MutationStatus::Error,
        MutationStatus::NotRun,
    ] {
        assert_eq!(
            resume_policy(Some(&stored("m", status))),
            ResumeDecision::Rerun
        );
    }
    assert_eq!(resume_policy(None), ResumeDecision::Rerun);
}

#[test]
fn fingerprint_is_canonical_for_set_like_fields() {
    let mut reordered = fixture_input();
    reordered.sources.reverse();
    reordered.targets.reverse();
    reordered.operators.reverse();
    assert_eq!(fingerprint(&fixture_input()), fingerprint(&reordered));
}

#[test]
fn fingerprint_inputs_are_canonical_and_compatibility_relevant() {
    let original = fixture_input();
    let mut reordered = original.clone();
    reordered.fingerprint_inputs.reverse();
    assert_eq!(fingerprint(&original), fingerprint(&reordered));

    let mut changed_hash = original.clone();
    changed_hash.fingerprint_inputs[0]
        .hash
        .replace_range(0..1, "f");
    assert_ne!(fingerprint(&original), fingerprint(&changed_hash));

    let mut changed_path = original.clone();
    changed_path.fingerprint_inputs[0].path = "pyproject.changed.toml".into();
    assert_ne!(fingerprint(&original), fingerprint(&changed_path));

    let mut added = original.clone();
    added.fingerprint_inputs.push(FingerprintInputFile {
        path: "fixtures/additional.json".into(),
        hash: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_owned(),
    });
    assert_ne!(fingerprint(&original), fingerprint(&added));

    let mut removed = original.clone();
    removed.fingerprint_inputs.clear();
    assert_ne!(fingerprint(&original), fingerprint(&removed));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn fingerprint_is_invariant_under_independent_set_orderings(
        reverse_sources in any::<bool>(),
        reverse_targets in any::<bool>(),
        reverse_operators in any::<bool>(),
    ) {
        let original = fixture_input();
        let mut reordered = original.clone();
        if reverse_sources {
            reordered.sources.reverse();
        }
        if reverse_targets {
            reordered.targets.reverse();
        }
        if reverse_operators {
            reordered.operators.reverse();
        }

        prop_assert_eq!(fingerprint(&original), fingerprint(&reordered));
    }
}

fn hex_hash(hash: [u8; 32]) -> String {
    hash.into_iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").unwrap();
            output
        })
}

fn permuted_and_duplicated(input: &FingerprintInput, seeds: [u64; 4]) -> FingerprintInput {
    let mut value = input.clone();
    value.sources.push(value.sources[0].clone());
    value.targets.push(value.targets[0].clone());
    value.operators.push(value.operators[0].clone());
    value
        .fingerprint_inputs
        .push(value.fingerprint_inputs[0].clone());
    deterministic_shuffle(&mut value.sources, seeds[0]);
    deterministic_shuffle(&mut value.targets, seeds[1]);
    deterministic_shuffle(&mut value.operators, seeds[2]);
    deterministic_shuffle(&mut value.fingerprint_inputs, seeds[3]);
    value
}

fn deterministic_shuffle<T>(values: &mut [T], mut state: u64) {
    for upper in (1..values.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let index = usize::try_from(state % u64::try_from(upper + 1).unwrap()).unwrap();
        values.swap(upper, index);
    }
}

fn edited_pair(
    input: &FingerprintInput,
    edit: FingerprintEdit,
) -> (FingerprintInput, FingerprintInput) {
    let mut before = input.clone();
    let mut after = input.clone();
    match edit {
        FingerprintEdit::HashByte => after.sources[0].hash[0] ^= 1,
        FingerprintEdit::PathComponent => {
            after.sources[0].path = format!("changed/{}", before.sources[0].path).into();
        }
        FingerprintEdit::NumericLimit => {
            after.limits.max_mutants =
                NonZeroUsize::new(before.limits.max_mutants.get() + 1).unwrap();
        }
        FingerprintEdit::ArgvByte => match &mut after.test_argv[0] {
            CommandArg::Unix(bytes) => bytes[0] ^= 1,
            CommandArg::Windows(units) => units[0] ^= 1,
        },
        FingerprintEdit::ArgvFlavor => {
            let payload = vec![0x61, 0, 0xff];
            before.test_argv[0] = CommandArg::Unix(
                payload
                    .iter()
                    .map(|unit| u8::try_from(*unit).unwrap())
                    .collect(),
            );
            after.test_argv[0] = CommandArg::Windows(payload);
        }
        FingerprintEdit::Operator => after.operators.push("boolean_literal".to_owned()),
        FingerprintEdit::Source => after.sources.push(SourceHash {
            path: "added/source.py".into(),
            hash: [0xa5; 32],
        }),
        FingerprintEdit::Target => after.targets.push(TargetSlice {
            path: "added/target.py".into(),
            lines: vec![LineRange { start: 1, end: 2 }],
            symbols: vec!["added".to_owned()],
        }),
        FingerprintEdit::FingerprintInput => {
            after.fingerprint_inputs.push(FingerprintInputFile {
                path: "added/config.toml".into(),
                hash: "f".repeat(64),
            });
        }
    }
    after
        .sources
        .sort_by(|left, right| left.path.cmp(&right.path));
    after
        .targets
        .sort_by(|left, right| left.path.cmp(&right.path));
    after.operators.sort();
    after
        .fingerprint_inputs
        .sort_by(|left, right| left.path.cmp(&right.path));
    (before, after)
}

#[derive(Default)]
struct ModelEncoder(Vec<u8>);

impl ModelEncoder {
    fn field(&mut self, tag: u8, value: &[u8]) {
        self.0.push(tag);
        self.0
            .extend_from_slice(&u64::try_from(value.len()).unwrap().to_le_bytes());
        self.0.extend_from_slice(value);
    }

    fn number(&mut self, tag: u8, value: u64) {
        self.field(tag, &value.to_le_bytes());
    }

    fn duration(&mut self, tag: u8, value: Duration) {
        let mut bytes = value.as_secs().to_le_bytes().to_vec();
        bytes.extend_from_slice(&value.subsec_nanos().to_le_bytes());
        self.field(tag, &bytes);
    }
}

fn canonical_model(input: &FingerprintInput) -> Vec<u8> {
    let mut model = ModelEncoder::default();
    model.field(1, &canonical_set(input.sources.iter().map(model_source)));
    model.field(2, &canonical_set(input.targets.iter().map(model_target)));
    model.field(
        3,
        &canonical_set(
            input
                .operators
                .iter()
                .map(|value| value.as_bytes().to_vec()),
        ),
    );
    model.field(4, &model_argv(&input.test_argv));
    model.field(5, &model_limits(&input.limits));
    model.field(
        6,
        &[match input.resource_mode {
            ResourceMode::Hard => 0,
            ResourceMode::BestEffort => 1,
        }],
    );
    model.field(
        7,
        &[match input.profile {
            MutationProfile::Full => 0,
            MutationProfile::Focused => 1,
        }],
    );
    model.field(
        8,
        &canonical_set(input.fingerprint_inputs.iter().map(model_fingerprint_input)),
    );
    model.0
}

fn canonical_set(values: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    let values = values.into_iter().collect::<BTreeSet<_>>();
    let mut out = ModelEncoder::default();
    out.number(0, u64::try_from(values.len()).unwrap());
    for value in values {
        out.field(1, &value);
    }
    out.0
}

fn model_source(source: &SourceHash) -> Vec<u8> {
    let mut out = ModelEncoder::default();
    out.field(1, source.path.as_str().as_bytes());
    out.field(2, &source.hash);
    out.0
}

fn model_target(target: &TargetSlice) -> Vec<u8> {
    let mut out = ModelEncoder::default();
    out.field(1, target.path.as_str().as_bytes());
    let lines = target
        .lines
        .iter()
        .map(|line| {
            let mut bytes = line.start.to_le_bytes().to_vec();
            bytes.extend_from_slice(&line.end.to_le_bytes());
            bytes
        })
        .collect::<BTreeSet<_>>();
    out.field(2, &canonical_set(lines));
    out.field(
        3,
        &canonical_set(
            target
                .symbols
                .iter()
                .map(|symbol| symbol.as_bytes().to_vec()),
        ),
    );
    out.0
}

fn model_fingerprint_input(input: &FingerprintInputFile) -> Vec<u8> {
    let mut out = ModelEncoder::default();
    out.field(1, input.path.as_str().as_bytes());
    out.field(2, input.hash.as_bytes());
    out.0
}

fn model_argv(argv: &[CommandArg]) -> Vec<u8> {
    let mut out = ModelEncoder::default();
    out.number(0, u64::try_from(argv.len()).unwrap());
    for argument in argv {
        match argument {
            CommandArg::Unix(bytes) => out.field(1, bytes),
            CommandArg::Windows(units) => {
                let bytes = units
                    .iter()
                    .flat_map(|unit| unit.to_le_bytes())
                    .collect::<Vec<_>>();
                out.field(2, &bytes);
            }
        }
    }
    out.0
}

fn model_limits(limits: &hoimin_core::RunLimits) -> Vec<u8> {
    let mut out = ModelEncoder::default();
    out.number(2, limits.max_mutants.get() as u64);
    out.number(3, limits.max_candidates.get() as u64);
    out.duration(4, limits.analyzer_timeout.get());
    out.duration(5, limits.baseline_timeout.get());
    match limits.mutant_timeout {
        MutantTimeout::Auto => out.field(6, &[0]),
        MutantTimeout::Fixed(value) => {
            out.field(6, &[1]);
            out.duration(7, value.get());
        }
    }
    out.duration(8, limits.total_timeout.get());
    out.number(9, limits.max_memory.get());
    out.number(11, limits.max_copy_size.get());
    out.number(12, limits.max_processes.get() as u64);
    out.0
}

#[test]
fn duplicate_paths_are_order_independent_by_the_whole_element() {
    let mut original = fixture_input();
    original.sources.push(SourceHash {
        path: "src/a.py".into(),
        hash: [9; 32],
    });
    original.targets.push(TargetSlice {
        path: "src/a.py".into(),
        lines: vec![LineRange { start: 8, end: 9 }],
        symbols: vec!["omega".to_owned()],
    });
    let mut reordered = original.clone();
    reordered.sources.swap(0, 2);
    reordered.targets.swap(0, 2);

    assert_eq!(fingerprint(&original), fingerprint(&reordered));
}

#[test]
fn duplicate_source_and_target_entries_are_semantically_idempotent() {
    let original = fixture_input();
    let mut duplicated = original.clone();
    duplicated.sources.push(original.sources[0].clone());
    duplicated.targets.push(original.targets[0].clone());

    assert_eq!(fingerprint(&original), fingerprint(&duplicated));
}

#[test]
fn every_compatibility_field_changes_the_fingerprint() {
    let original = fixture_input();
    let expected = fingerprint(&original);
    let mut variants = Vec::new();

    let mut value = original.clone();
    value.sources[0].hash[0] ^= 1;
    variants.push(("source hash", value));
    let mut value = original.clone();
    value.targets[0].lines[0].end += 1;
    variants.push(("target", value));
    let mut value = original.clone();
    value.operators.push("comparison".to_owned());
    variants.push(("operator", value));
    let mut value = original.clone();
    value.test_argv.push(CommandArg::Unix(vec![0, 255]));
    variants.push(("argv", value));

    for (name, value) in variants {
        assert_ne!(expected, fingerprint(&value), "unchanged field: {name}");
    }
}

#[test]
fn fingerprint_changes_when_resource_mode_changes() {
    let mut changed = fixture_input();
    changed.resource_mode = ResourceMode::BestEffort;
    assert_ne!(fingerprint(&fixture_input()), fingerprint(&changed));
}

#[test]
fn fingerprint_changes_when_mutation_profile_changes() {
    let focused = FingerprintInput {
        profile: MutationProfile::Focused,
        ..fixture_input()
    };

    assert_ne!(fingerprint(&fixture_input()), fingerprint(&focused));
}

#[test]
fn fingerprint_changes_when_type_operator_selection_changes() {
    let changed = FingerprintInput {
        operators: vec!["type_nullable_remove".to_owned()],
        ..fixture_input()
    };
    assert_ne!(fingerprint(&fixture_input()), fingerprint(&changed));
}

#[test]
fn worker_concurrency_does_not_change_resume_compatibility() {
    let original = fixture_input();
    let mut raw = fixture_raw_limits();
    mutate_jobs(&mut raw);
    let mut changed = original.clone();
    changed.limits = (&raw).try_into().unwrap();

    assert_eq!(fingerprint(&original), fingerprint(&changed));
}

#[test]
fn output_retention_does_not_change_resume_compatibility() {
    let original = fixture_input();
    let mut raw = fixture_raw_limits();
    mutate_max_output(&mut raw);
    let mut changed = original.clone();
    changed.limits = (&raw).try_into().unwrap();

    assert_eq!(fingerprint(&original), fingerprint(&changed));
}

#[test]
fn every_verdict_or_safety_limit_changes_the_fingerprint() {
    let original = fixture_input();
    let expected = fingerprint(&original);
    for (name, mutate) in [
        ("max_mutants", mutate_max_mutants as fn(&mut RawRunLimits)),
        ("max_candidates", mutate_max_candidates),
        ("analyzer_timeout", mutate_analyzer_timeout),
        ("baseline_timeout", mutate_baseline_timeout),
        ("mutant_timeout", mutate_mutant_timeout),
        ("total_timeout", mutate_total_timeout),
        ("max_memory", mutate_max_memory),
        ("max_copy_size", mutate_max_copy_size),
        ("max_workspace_size", mutate_max_workspace_size),
        ("min_free_space", mutate_min_free_space),
        ("max_processes", mutate_max_processes),
    ] {
        let mut raw = fixture_raw_limits();
        mutate(&mut raw);
        let mut value = original.clone();
        value.limits = (&raw).try_into().unwrap();
        assert_ne!(expected, fingerprint(&value), "unchanged limit: {name}");
    }
}

#[test]
fn command_arguments_preserve_native_units_exactly() {
    let mut unix = fixture_input();
    unix.test_argv = vec![CommandArg::Unix(vec![0x61, 0, 0xff])];
    let mut windows = unix.clone();
    windows.test_argv = vec![CommandArg::Windows(vec![0x61, 0, 0xff])];
    let mut windows_other = windows.clone();
    windows_other.test_argv = vec![CommandArg::Windows(vec![0x61, 0, 0x100])];
    assert_ne!(fingerprint(&unix), fingerprint(&windows));
    assert_ne!(fingerprint(&windows), fingerprint(&windows_other));
}

#[derive(Clone, Copy, Debug)]
enum FingerprintEdit {
    HashByte,
    PathComponent,
    NumericLimit,
    ArgvByte,
    ArgvFlavor,
    Operator,
    Source,
    Target,
    FingerprintInput,
}

fn unicode_component() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec!['a', 'z', 'é', '雪', '中', 'λ', '🧪', '_', '-']),
        1..=8,
    )
    .prop_map(|characters| characters.into_iter().collect())
}

fn arbitrary_sources() -> impl Strategy<Value = Vec<SourceHash>> {
    prop::collection::vec((unicode_component(), any::<[u8; 32]>()), 1..=5).prop_map(|values| {
        let mut sources = values
            .into_iter()
            .enumerate()
            .map(|(index, (component, hash))| SourceHash {
                path: format!("src/{component}-{index}.py").into(),
                hash,
            })
            .collect::<Vec<_>>();
        sources.sort_by(|left, right| left.path.cmp(&right.path));
        sources
    })
}

fn arbitrary_targets() -> impl Strategy<Value = Vec<TargetSlice>> {
    prop::collection::vec(
        (
            unicode_component(),
            prop::collection::vec((1_u32..10, 0_u32..10), 0..=4),
            prop::collection::vec(unicode_component(), 0..=4),
        ),
        1..=5,
    )
    .prop_map(|values| {
        let mut targets = values
            .into_iter()
            .enumerate()
            .map(|(index, (component, gaps_and_widths, symbols))| {
                let mut previous_end = 0;
                let lines = gaps_and_widths
                    .into_iter()
                    .map(|(gap, width)| {
                        let start = previous_end + gap;
                        let end = start + width;
                        previous_end = end;
                        LineRange { start, end }
                    })
                    .collect::<Vec<_>>();
                let symbols = symbols
                    .into_iter()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                TargetSlice {
                    path: format!("src/{component}-{index}.py").into(),
                    lines,
                    symbols,
                }
            })
            .collect::<Vec<_>>();
        targets.sort_by(|left, right| left.path.cmp(&right.path));
        targets
    })
}

fn arbitrary_operators() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop::sample::select(vec![
            "binary_add_sub",
            "compare_eq_ne",
            "type_nullable_remove",
        ]),
        1..=5,
    )
    .prop_map(|values| {
        values
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    })
}

fn arbitrary_fingerprint_inputs() -> impl Strategy<Value = Vec<FingerprintInputFile>> {
    prop::collection::vec((unicode_component(), any::<[u8; 32]>()), 1..=5).prop_map(|values| {
        let mut inputs = values
            .into_iter()
            .enumerate()
            .map(|(index, (component, hash))| FingerprintInputFile {
                path: format!("config/{component}-{index}.toml").into(),
                hash: hex_hash(hash),
            })
            .collect::<Vec<_>>();
        inputs.sort_by(|left, right| left.path.cmp(&right.path));
        inputs
    })
}

fn arbitrary_argv() -> impl Strategy<Value = Vec<CommandArg>> {
    prop::collection::vec(
        prop_oneof![
            prop::collection::vec(any::<u8>(), 1..=8).prop_map(CommandArg::Unix),
            prop::collection::vec(any::<u16>(), 1..=8).prop_map(CommandArg::Windows),
        ],
        1..=5,
    )
}

fn arbitrary_limits() -> impl Strategy<Value = hoimin_core::RunLimits> {
    (
        1_usize..=8,
        1_usize..=128,
        1_usize..=256,
        1_u64..=30,
        1_u64..=60,
        prop::option::of(1_u64..=30),
        1_u64..=300,
        1_u64..=1_000_000,
        1_u64..=100_000,
        1_u64..=1_000_000,
        8_usize..=64,
    )
        .prop_map(
            |(
                jobs,
                max_mutants,
                max_candidates,
                analyzer,
                baseline,
                mutant,
                total,
                memory,
                output,
                copy,
                processes,
            )| {
                (&RawRunLimits {
                    jobs,
                    max_mutants,
                    max_candidates,
                    analyzer_timeout: Duration::from_secs(analyzer),
                    baseline_timeout: Duration::from_secs(baseline),
                    mutant_timeout: mutant.map(Duration::from_secs),
                    total_timeout: Duration::from_secs(total),
                    max_memory: memory,
                    max_output: output,
                    max_copy_size: copy,
                    max_workspace_size: 8 * 1024 * 1024 * 1024,
                    min_free_space: 10 * 1024 * 1024 * 1024,
                    max_processes: processes,
                })
                    .try_into()
                    .unwrap()
            },
        )
}

fn arbitrary_fingerprint_input() -> impl Strategy<Value = FingerprintInput> {
    (
        arbitrary_sources(),
        arbitrary_fingerprint_inputs(),
        arbitrary_targets(),
        arbitrary_operators(),
        arbitrary_argv(),
        arbitrary_limits(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(
                sources,
                fingerprint_inputs,
                targets,
                operators,
                test_argv,
                limits,
                focused,
                best,
            )| {
                FingerprintInput {
                    sources,
                    fingerprint_inputs,
                    targets,
                    operators,
                    profile: if focused {
                        MutationProfile::Focused
                    } else {
                        MutationProfile::Full
                    },
                    test_argv,
                    limits,
                    resource_mode: if best {
                        ResourceMode::BestEffort
                    } else {
                        ResourceMode::Hard
                    },
                }
            },
        )
}

fn targets_are_independently_normalized(targets: &[TargetSlice]) -> bool {
    targets.windows(2).all(|pair| pair[0].path < pair[1].path)
        && targets.iter().all(|target| {
            target
                .lines
                .iter()
                .all(|range| range.start > 0 && range.start <= range.end)
                && target
                    .lines
                    .windows(2)
                    .all(|pair| pair[0].end < pair[1].start)
                && target.symbols.windows(2).all(|pair| pair[0] < pair[1])
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn generated_targets_are_independently_normalized(targets in arbitrary_targets()) {
        prop_assert!(targets_are_independently_normalized(&targets));
    }

    #[test]
    fn fingerprint_is_invariant_under_set_permutations_and_duplicates(
        input in arbitrary_fingerprint_input(),
        seeds in any::<[u64; 4]>(),
    ) {
        prop_assert!(targets_are_independently_normalized(&input.targets));
        let variant = permuted_and_duplicated(&input, seeds);
        prop_assert_eq!(canonical_model(&input), canonical_model(&variant));
        prop_assert_eq!(fingerprint(&input), fingerprint(&variant));
    }

    #[test]
    fn every_canonical_edit_changes_the_fingerprint(
        input in arbitrary_fingerprint_input(),
        edit in prop::sample::select(vec![
            FingerprintEdit::HashByte,
            FingerprintEdit::PathComponent,
            FingerprintEdit::NumericLimit,
            FingerprintEdit::ArgvByte,
            FingerprintEdit::ArgvFlavor,
            FingerprintEdit::Operator,
            FingerprintEdit::Source,
            FingerprintEdit::Target,
            FingerprintEdit::FingerprintInput,
        ]),
    ) {
        prop_assert!(targets_are_independently_normalized(&input.targets));
        let (before, after) = edited_pair(&input, edit);
        let before_model = canonical_model(&before);
        let after_model = canonical_model(&after);
        prop_assume!(before_model != after_model);
        prop_assert_ne!(fingerprint(&before), fingerprint(&after));
    }
}

fn fixture_input() -> FingerprintInput {
    FingerprintInput {
        sources: vec![
            SourceHash {
                path: "src/a.py".into(),
                hash: [1; 32],
            },
            SourceHash {
                path: "src/b.py".into(),
                hash: [2; 32],
            },
        ],
        targets: vec![
            TargetSlice {
                path: "src/a.py".into(),
                lines: vec![LineRange { start: 1, end: 3 }],
                symbols: vec!["alpha".to_owned()],
            },
            TargetSlice {
                path: "src/b.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            },
        ],
        operators: vec!["boolean".to_owned(), "binary".to_owned()],
        profile: MutationProfile::Full,
        test_argv: vec![CommandArg::Unix(vec![0xff, 0, b'x'])],
        limits: (&fixture_raw_limits()).try_into().unwrap(),
        resource_mode: ResourceMode::Hard,
        fingerprint_inputs: vec![
            FingerprintInputFile {
                path: "pyproject.toml".into(),
                hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            },
            FingerprintInputFile {
                path: "fixtures/case.json".into(),
                hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
            },
        ],
    }
}

fn fixture_raw_limits() -> RawRunLimits {
    RawRunLimits {
        mutant_timeout: Some(Duration::from_secs(7)),
        ..RawRunLimits::default()
    }
}

fn stored(id: &str, status: MutationStatus) -> StoredResult {
    StoredResult {
        mutant_id: id.to_owned(),
        status,
    }
}

fn mutate_jobs(v: &mut RawRunLimits) {
    v.jobs += 1;
}
fn mutate_max_mutants(v: &mut RawRunLimits) {
    v.max_mutants += 1;
}
fn mutate_max_candidates(v: &mut RawRunLimits) {
    v.max_candidates += 1;
}
fn mutate_analyzer_timeout(v: &mut RawRunLimits) {
    v.analyzer_timeout += Duration::from_secs(1);
}
fn mutate_baseline_timeout(v: &mut RawRunLimits) {
    v.baseline_timeout += Duration::from_secs(1);
}
fn mutate_mutant_timeout(v: &mut RawRunLimits) {
    v.mutant_timeout = Some(Duration::from_secs(8));
}
fn mutate_total_timeout(v: &mut RawRunLimits) {
    v.total_timeout += Duration::from_secs(1);
}
fn mutate_max_memory(v: &mut RawRunLimits) {
    v.max_memory += 1;
}
fn mutate_max_output(v: &mut RawRunLimits) {
    v.max_output += 1;
}
fn mutate_max_copy_size(v: &mut RawRunLimits) {
    v.max_copy_size += 1;
}
fn mutate_max_processes(v: &mut RawRunLimits) {
    v.max_processes += 1;
}
fn mutate_max_workspace_size(v: &mut RawRunLimits) {
    v.max_workspace_size += 1;
}
fn mutate_min_free_space(v: &mut RawRunLimits) {
    v.min_free_space += 1;
}
