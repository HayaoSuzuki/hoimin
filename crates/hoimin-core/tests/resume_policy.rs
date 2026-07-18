use std::collections::BTreeSet;
use std::time::Duration;

use hoimin_core::{
    CommandArg, FingerprintInput, LineRange, MutationStatus, RawRunLimits, ResourceMode,
    RunFingerprint, SourceHash, StoredResult, StoredRun, TargetSlice, fingerprint, resume_policy,
    select_resume_run,
};

#[test]
fn resume_reuses_only_determinate_completed_results() {
    let decision = resume_policy(&[
        stored("m1", MutationStatus::Killed),
        stored("m2", MutationStatus::Survived),
        stored("m3", MutationStatus::Timeout),
        stored("m4", MutationStatus::OutOfMemory),
        stored("m5", MutationStatus::ProcessLimit),
        stored("m6", MutationStatus::Error),
        stored("m7", MutationStatus::NotRun),
    ]);
    assert_eq!(
        decision.reusable_ids,
        BTreeSet::from(["m1".to_owned(), "m2".to_owned()])
    );
    assert_eq!(
        decision.rerun_ids,
        BTreeSet::from([
            "m3".to_owned(),
            "m4".to_owned(),
            "m5".to_owned(),
            "m6".to_owned(),
            "m7".to_owned(),
        ])
    );
}

#[test]
fn newest_compatible_incomplete_run_is_selected_and_complete_runs_are_ignored() {
    let wanted = fingerprint(&fixture_input());
    let other = RunFingerprint::from_bytes([9; 32]);
    let runs = vec![
        run("old", wanted, 1, false, "m-old"),
        run("complete", wanted, 99, true, "m-complete"),
        run("other", other, 100, false, "m-other"),
        run("new", wanted, 2, false, "m-new"),
    ];
    let decision = select_resume_run(&runs, &wanted).unwrap();
    assert_eq!(decision.run_id.as_deref(), Some("new"));
    assert_eq!(decision.reusable_ids, BTreeSet::from(["m-new".to_owned()]));
    assert!(select_resume_run(&runs, &RunFingerprint::from_bytes([8; 32])).is_none());
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
    let mut value = original.clone();
    value.python_version.push_str(".1");
    variants.push(("Python", value));
    let mut value = original.clone();
    value.libcst_version.push_str(".1");
    variants.push(("LibCST", value));
    let mut value = original.clone();
    value.resource_mode = ResourceMode::BestEffort;
    variants.push(("resource mode", value));

    for (name, value) in variants {
        assert_ne!(expected, fingerprint(&value), "unchanged field: {name}");
    }
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
        python_version: "3.13.4".to_owned(),
        libcst_version: "1.8.2".to_owned(),
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

fn run(
    run_id: &str,
    fingerprint: RunFingerprint,
    ordinal: u64,
    complete: bool,
    mutant: &str,
) -> StoredRun {
    StoredRun {
        run_id: run_id.to_owned(),
        fingerprint,
        ordinal,
        complete,
        results: vec![stored(mutant, MutationStatus::Killed)],
        diagnostics: Vec::new(),
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
