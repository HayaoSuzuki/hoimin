use std::time::Duration;

use hoimin_core::{
    CommandArg, FingerprintInput, LineRange, MutationStatus, RawRunLimits, ResourceMode,
    ResumeDecision, SourceHash, StoredResult, TargetSlice, fingerprint, resume_policy,
};
use proptest::prelude::*;

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

proptest! {
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
fn fingerprint_changes_when_type_operator_selection_changes() {
    let changed = FingerprintInput {
        operators: vec!["type_nullable_remove".to_owned()],
        ..fixture_input()
    };
    assert_ne!(fingerprint(&fixture_input()), fingerprint(&changed));
}
#[test]
fn every_safety_limit_changes_the_fingerprint() {
    let original = fixture_input();
    let expected = fingerprint(&original);
    for (name, mutate) in [
        ("jobs", mutate_jobs as fn(&mut RawRunLimits)),
        ("max_mutants", mutate_max_mutants),
        ("max_candidates", mutate_max_candidates),
        ("analyzer_timeout", mutate_analyzer_timeout),
        ("baseline_timeout", mutate_baseline_timeout),
        ("mutant_timeout", mutate_mutant_timeout),
        ("total_timeout", mutate_total_timeout),
        ("max_memory", mutate_max_memory),
        ("max_output", mutate_max_output),
        ("max_copy_size", mutate_max_copy_size),
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
        test_argv: vec![CommandArg::Unix(vec![0xff, 0, b'x'])],
        limits: (&fixture_raw_limits()).try_into().unwrap(),
        resource_mode: ResourceMode::Hard,
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
