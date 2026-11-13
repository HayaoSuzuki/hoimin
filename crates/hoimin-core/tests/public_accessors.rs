use hoimin_core::{
    MutationProfile, ProcessOutputState, VerificationSelectionMode, VerificationSelectionPolicy,
    VerificationSelectionScope,
};

#[rstest::rstest]
#[case(MutationProfile::Full, "full")]
#[case(MutationProfile::Focused, "focused")]
fn profile_names_match_serialized_strings(#[case] profile: MutationProfile, #[case] name: &str) {
    assert_eq!(profile.as_str(), name);
    assert_eq!(serde_json::to_value(profile).unwrap(), name);
}

#[rstest::rstest]
#[case(VerificationSelectionMode::CandidateIds, "candidate_ids")]
#[case(VerificationSelectionMode::Top, "top")]
#[case(VerificationSelectionMode::Sample, "sample")]
fn mode_names_match_serialized_strings(
    #[case] mode: VerificationSelectionMode,
    #[case] name: &str,
) {
    assert_eq!(mode.as_str(), name);
    assert_eq!(serde_json::to_value(mode).unwrap(), name);
}

#[rstest::rstest]
#[case(VerificationSelectionPolicy::ExplicitCandidates, "explicit_candidates")]
#[case(VerificationSelectionPolicy::Strict, "strict")]
#[case(VerificationSelectionPolicy::FileRoundRobinV1, "file_round_robin_v1")]
#[case(VerificationSelectionPolicy::LineRoundRobinV1, "line_round_robin_v1")]
#[case(
    VerificationSelectionPolicy::Splitmix64FisherYatesV1,
    "splitmix64_fisher_yates_v1"
)]
fn policy_names_match_serialized_strings(
    #[case] policy: VerificationSelectionPolicy,
    #[case] name: &str,
) {
    assert_eq!(policy.as_str(), name);
    assert_eq!(serde_json::to_value(policy).unwrap(), name);
}

#[rstest::rstest]
#[case(VerificationSelectionScope::ExplicitCandidates, "explicit_candidates")]
#[case(VerificationSelectionScope::RetainedCandidates, "retained_candidates")]
#[case(VerificationSelectionScope::SampledCandidates, "sampled_candidates")]
fn scope_names_match_serialized_strings(
    #[case] scope: VerificationSelectionScope,
    #[case] name: &str,
) {
    assert_eq!(scope.as_str(), name);
    assert_eq!(serde_json::to_value(scope).unwrap(), name);
}
#[rstest::rstest]
#[case(ProcessOutputState::Complete, true)]
#[case(ProcessOutputState::CloseTimedOut, false)]
fn process_output_completion(#[case] state: ProcessOutputState, #[case] expected: bool) {
    assert_eq!(state.is_complete(), expected);
}

#[test]
fn ledger_exposes_the_limits_it_enforces() {
    let limits = hoimin_core::RunBudgets {
        memory: 17,
        copy: 23,
        processes: 5,
    };
    assert_eq!(hoimin_core::BudgetLedger::new(limits).limits(), limits);
}

use hoimin_core::{ConfigError, EffectFailure, EffectId, RawRunLimits, ReservationId, RunLimits};

#[rstest::rstest]
#[case(&EffectFailure::WorkspaceRestore { path: "src.py".into(), message: "denied".into() }, "workspace.restore", "restore src.py: denied")]
#[case(&EffectFailure::WorkspacePreflightMismatch { expected: EffectId(1), received: EffectId(2) }, "workspace.preflight.mismatch", "preflight effect mismatch: expected EffectId(1), received EffectId(2)")]
#[case(&EffectFailure::WorkspaceWorkerOutOfRange { worker: 3, requested_workers: 2 }, "workspace.worker.out_of_range", "worker 3 is outside requested count 2")]
#[case(&EffectFailure::WorkspaceAllowanceMismatch { expected: 17, received: 19 }, "workspace.allowance.mismatch", "workspace allowance mismatch: expected 17, received 19")]
#[case(&EffectFailure::InvalidWorkspaceGrant { expected: ReservationId(3), received: ReservationId(4) }, "workspace.grant.invalid", "workspace grant mismatch: expected ReservationId(3), received ReservationId(4)")]
#[case(&EffectFailure::WorkerMissing { worker: 5 }, "workspace.worker.missing", "worker 5 is missing")]
#[case(&EffectFailure::ReportSerialization { message: "invalid event".into() }, "report.serialization", "invalid event")]
#[case(&EffectFailure::ReportState { message: "duplicate start".into() }, "report.state", "duplicate start")]
#[case(&EffectFailure::OriginalChanged { path: "src.py".into() }, "workspace.original.changed", "original changed: src.py")]
#[case(&EffectFailure::CopyLimit { requested: 17, allowance: 11 }, "workspace.copy.limit", "copy requires 17 bytes, allowance is 11")]
#[case(&EffectFailure::InvalidWorkspacePath { path: "../src.py".into() }, "workspace.path.invalid", "invalid workspace path: ../src.py")]
#[case(&EffectFailure::InvalidMutation { code: "invalid.mutation".into(), path: "src.py".into(), message: "bad span".into() }, "invalid.mutation", "invalid mutation for src.py: bad span")]
#[case(&EffectFailure::Io { code: "read.failed".into(), operation: "read".into(), path: Some("src.py".into()), message: "denied".into() }, "read.failed", "read src.py: denied")]
#[case(&EffectFailure::Io { code: "read.failed".into(), operation: "read".into(), path: None, message: "denied".into() }, "read.failed", "read: denied")]
fn workspace_and_report_failures_retain_their_specific_diagnostics(
    #[case] failure: &EffectFailure,
    #[case] code: &str,
    #[case] message: &str,
) {
    assert_eq!(failure.code(), code);
    assert_eq!(failure.message(), message);
}

#[rstest::rstest]
#[case(&RawRunLimits { max_processes: 0, ..RawRunLimits::default() }, "max_processes")]
#[case(&RawRunLimits { max_mutants: 0, ..RawRunLimits::default() }, "max_mutants")]
#[case(&RawRunLimits { max_candidates: 0, ..RawRunLimits::default() }, "max_candidates")]
#[case(&RawRunLimits { max_memory: 0, ..RawRunLimits::default() }, "max_memory")]
#[case(&RawRunLimits { max_output: 0, ..RawRunLimits::default() }, "max_output")]
#[case(&RawRunLimits { max_copy_size: 0, ..RawRunLimits::default() }, "max_copy_size")]
fn zero_resource_limits_reject_the_specific_field(
    #[case] raw: &RawRunLimits,
    #[case] field: &'static str,
) {
    assert_eq!(
        RunLimits::try_from(raw),
        Err(ConfigError::InvalidLimit(field))
    );
    assert_eq!(
        ConfigError::InvalidLimit(field).to_string(),
        format!(
            "invalid zero or overflowing limit: --{}",
            field.replace('_', "-")
        )
    );
}

#[test]
fn extension_limit_diagnostic_preserves_its_name() {
    assert_eq!(
        ConfigError::InvalidLimit("extension_limit").to_string(),
        "invalid zero or overflowing limit: extension_limit"
    );
}

#[test]
fn disabled_session_has_no_mutable_configuration() {
    assert!(hoimin_core::SessionMode::Disabled.config_mut().is_none());
}

#[rstest::rstest]
#[case(String::new())]
#[case("x".repeat(4097))]
fn aggregate_disk_probe_requires_bounded_nonempty_evidence(#[case] observation: String) {
    use hoimin_core::{DiskCapabilityProbe, DiskEnforcementError, DiskEnforcementReport};
    assert_eq!(
        DiskEnforcementReport::verified_aggregate(
            "linux_project_quota".into(),
            DiskCapabilityProbe {
                capability: "project_quota".into(),
                verified: true,
                observation
            }
        ),
        Err(DiskEnforcementError::InvalidObservation)
    );
}

#[test]
fn source_paths_must_stay_inside_the_project() {
    use hoimin_core::{Selection, TargetError, resolve_explicit};
    let outside = Selection {
        root: "/project".into(),
        sources: vec!["../escape".into()],
        ..Selection::default()
    };
    assert_eq!(
        resolve_explicit(&outside, &[]),
        Err(TargetError::PathOutsideRoot("../escape".into()))
    );
}

#[test]
fn line_paths_must_stay_inside_the_project() {
    use hoimin_core::{LineRange, LineSelection, Selection, TargetError, resolve_explicit};
    let outside = Selection {
        root: "/project".into(),
        lines: vec![LineSelection {
            path: "../escape.py".into(),
            range: LineRange { start: 1, end: 1 },
        }],
        ..Selection::default()
    };
    assert_eq!(
        resolve_explicit(&outside, &[]),
        Err(TargetError::PathOutsideRoot("../escape.py".into()))
    );
}

#[test]
fn symbol_selection_resolves_the_discovered_module() {
    use hoimin_core::{DiscoveredFile, Selection, resolve_explicit};
    let selected = Selection {
        root: "/project".into(),
        symbols: vec![hoimin_core::SymbolSelection {
            module: "calc".into(),
            qualname: "f".into(),
        }],
        sources: vec![".".into()],
        ..Selection::default()
    };
    let targets = resolve_explicit(&selected, &[DiscoveredFile::python("calc.py")]).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, "calc.py");
}

#[test]
fn changed_only_selection_admits_the_changed_lines() {
    use hoimin_core::{LineRange, TargetSlice, intersect_changed};
    let changed = std::collections::BTreeMap::from([(
        "calc.py".into(),
        vec![LineRange { start: 2, end: 4 }],
    )]);
    assert_eq!(
        intersect_changed(&[], &changed),
        vec![TargetSlice {
            path: "calc.py".into(),
            lines: vec![LineRange { start: 2, end: 4 }],
            symbols: vec![]
        }]
    );
}

#[rstest::rstest]
#[case(|raw: &mut hoimin_core::RawRunConfig| raw.test_argv.clear(), "at least one test argv")]
#[case(|raw: &mut hoimin_core::RawRunConfig| raw.exclude_operators = vec!["unknown".into()], "unknown mutation operator")]
#[case(|raw: &mut hoimin_core::RawRunConfig| raw.fingerprint_env = vec!["INVALID=NAME".into()], "--fingerprint-env")]
#[case(|raw: &mut hoimin_core::RawRunConfig| raw.operators = vec!["unknown".into()], "unknown mutation operator")]
#[case(|raw: &mut hoimin_core::RawRunConfig| raw.import_roots = vec!["../escape".into()], "--import-root")]
fn raw_configuration_propagates_invalid_nested_settings(
    #[case] mutate: fn(&mut hoimin_core::RawRunConfig),
    #[case] expected: &str,
) {
    let mut raw = hoimin_core::RawRunConfig {
        root: "/project".into(),
        sources: vec!["src".into()],
        test_argv: vec![hoimin_core::CommandArg::Unix(b"python".to_vec())],
        ..hoimin_core::RawRunConfig::default()
    };
    mutate(&mut raw);
    assert!(
        hoimin_core::RunConfig::try_from(raw)
            .unwrap_err()
            .to_string()
            .contains(expected)
    );
}

#[test]
fn environment_request_requires_a_list_of_names() {
    let error = serde_json::from_str::<hoimin_core::EnvironmentRequest>("42").unwrap_err();
    assert!(error.to_string().contains("expected a sequence"));
}

#[rstest::rstest]
#[case("analyzer_timeout", "--analyzer-timeout")]
#[case("total_timeout", "--total-timeout")]
fn timeout_limit_diagnostics_use_the_cli_flag(#[case] field: &'static str, #[case] flag: &str) {
    assert_eq!(
        ConfigError::InvalidLimit(field).to_string(),
        format!("invalid zero or overflowing limit: {flag}")
    );
}

#[test]
fn output_replacement_keeps_validated_execution_settings() {
    let raw = hoimin_core::RawRunConfig {
        root: "/project".into(),
        sources: vec!["src".into()],
        test_argv: vec![hoimin_core::CommandArg::Unix(b"python".to_vec())],
        ..hoimin_core::RawRunConfig::default()
    };
    let config = hoimin_core::RunConfig::try_from(raw).unwrap();
    let before = config.clone();
    let output = hoimin_core::OutputConfig {
        metrics: Some("metrics.json".into()),
    };
    let updated = config.with_output(output.clone());
    assert_eq!(updated.output, output);
    assert_eq!(updated.selection, before.selection);
    assert_eq!(updated.limits, before.limits);
    assert_eq!(updated.test_argv, before.test_argv);
}

#[rstest::rstest]
#[case(hoimin_core::JanitorReason::Young, true)]
#[case(hoimin_core::JanitorReason::ActiveLease, true)]
#[case(hoimin_core::JanitorReason::RetainMarker, true)]
#[case(hoimin_core::JanitorReason::CoordinatorBusy, true)]
#[case(hoimin_core::JanitorReason::MutationBarrierActive, true)]
#[case(hoimin_core::JanitorReason::DeleteFailed, false)]
fn janitor_distinguishes_normal_holds_from_cleanup_failures(
    #[case] reason: hoimin_core::JanitorReason,
    #[case] expected: bool,
) {
    assert_eq!(reason.is_normal_hold(), expected);
}

#[test]
fn resumed_outcome_decodes_without_redundant_fields() {
    assert_eq!(
        serde_json::from_str::<hoimin_core::ResumeOutcome>(r#"{"status":"resumed"}"#).unwrap(),
        hoimin_core::ResumeOutcome::Resumed
    );
}

#[rstest::rstest]
#[case(
    &hoimin_core::WorkspaceBudgetError::AggregateOverflow,
    "workspace.preflight.aggregate_overflow"
)]
#[case(&hoimin_core::WorkspaceBudgetError::AggregateMismatch { per_worker: 1, workers: 2, reported: 1 }, "workspace.preflight.aggregate_mismatch")]
#[case(&hoimin_core::WorkspaceBudgetError::WorkerOutOfRange { worker: 2, requested_workers: 1 }, "workspace.worker.out_of_range")]
#[case(&hoimin_core::WorkspaceBudgetError::Reserve(hoimin_core::ReserveError::LimitReached(hoimin_core::LimitReached { kind: hoimin_core::BudgetKind::Copy, requested: 2, available: 1 })), "workspace.copy.limit")]
#[case(
    &hoimin_core::WorkspaceBudgetError::Reserve(hoimin_core::ReserveError::ReservationIdsExhausted),
    "workspace.reservation_id.exhausted"
)]
fn workspace_budget_failures_have_specific_codes(
    #[case] error: &hoimin_core::WorkspaceBudgetError,
    #[case] code: &str,
) {
    assert_eq!(error.code(), code);
}

#[test]
fn persisted_plan_rejects_non_normalized_import_roots() {
    let raw = hoimin_core::RawRunConfig {
        root: "/project".into(),
        sources: vec!["src".into()],
        test_argv: vec![hoimin_core::CommandArg::Unix(b"python".to_vec())],
        ..hoimin_core::RawRunConfig::default()
    };
    let mut plan = hoimin_core::RunConfig::try_from(raw)
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap()
        .into_plan_config(
            hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(1)).unwrap(),
        );
    plan.import_roots = vec!["src/./pkg".into()];
    assert_eq!(plan.validate(), Err(ConfigError::NonNormalizedImportRoots));
}

#[test]
fn environment_request_decodes_a_valid_list_of_names() {
    let request = serde_json::from_str::<hoimin_core::EnvironmentRequest>(r#"["PATH"]"#).unwrap();
    assert_eq!(request.names(), ["PATH"]);
}

#[test]
fn baseline_event_accessors_read_the_stored_header_fields() {
    let event = hoimin_core::OutputEvent::BaselineFinished(hoimin_core::BaselineFinished {
        schema_version: 123,
        sequence: 7,
        run_id: "baseline-run".into(),
        termination: hoimin_core::ProcessTermination::Exit(0),
        elapsed_ms: 1,
        resource_mode: hoimin_core::ResourceMode::Hard,
        output: hoimin_core::OutputSpoolRef {
            token: "output".into(),
            retained: 0,
            observed: 0,
        },
    });
    assert_eq!(event.schema_version(), 123);
    assert_eq!(event.sequence(), 7);
    assert_eq!(event.run_id(), "baseline-run");
}

#[test]
fn logical_path_key_uses_native_case_rules() {
    let key = hoimin_core::logical_path_equality_key(camino::Utf8Path::new("src/calc.py"));
    #[cfg(not(windows))]
    assert_eq!(key, "src/calc.py");
    #[cfg(windows)]
    assert_eq!(key, "SRC/CALC.PY");
}

#[test]
fn line_selection_requires_a_discovered_python_file() {
    let selected = hoimin_core::Selection {
        root: "/project".into(),
        lines: vec![hoimin_core::LineSelection {
            path: "missing.py".into(),
            range: hoimin_core::LineRange { start: 1, end: 1 },
        }],
        ..hoimin_core::Selection::default()
    };
    assert_eq!(
        hoimin_core::resolve_explicit(&selected, &[]),
        Err(hoimin_core::TargetError::MissingOrNonPythonFile(
            "missing.py".into()
        ))
    );
}
