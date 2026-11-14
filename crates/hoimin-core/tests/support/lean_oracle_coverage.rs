use super::*;

#[rstest::rstest]
#[case(|case: &mut OracleCase| case.schedule.clear(), "no executable steps")]
#[case(|case: &mut OracleCase| case.schedule.push("cancel".into()), "scheduled events")]
#[case(|case: &mut OracleCase| case.expected[0].event = "different".into(), "does not match schedule")]
#[case(|case: &mut OracleCase| case.expected[0].verdict = "unknown".into(), "unknown verdict")]
#[case(|case: &mut OracleCase| case.expected[0].error_code = Some("unknown".into()), "unknown error code")]
fn malformed_oracle_cases_are_rejected_before_execution(
    #[case] invalidate: fn(&mut OracleCase),
    #[case] message: &str,
) {
    let mut case = load_corpus().unwrap().remove(0);
    invalidate(&mut case);

    let error = validate_case(&case).unwrap_err();

    let actual = error.contains(message);

    assert!(actual);
}

#[test]
fn empty_corpus_is_rejected() {
    let result = parse_corpus("\n\n");

    let actual = result.unwrap_err().contains("no cases");

    assert!(actual);
}

#[test]
fn whitespace_separated_corpus_is_accepted() {
    let corpus = format!("\n{}\n\n", corpus_text());

    let cases = parse_corpus(&corpus).unwrap();

    let actual = cases.len();

    assert_eq!(actual, 22);
}

#[rstest::rstest]
fn mismatched_observations_report_every_changed_field(
    #[values("verdict", "error_code", "phase", "emitted", "pending")] field: &str,
) {
    let expected = Observation {
        verdict: "accepted".into(),
        error_code: None,
        phase: "running".into(),
        emitted: vec!["ordinary".into()],
        pending: 1,
    };
    let actual = Observation {
        verdict: "rejected".into(),
        error_code: Some("machine.effect.unknown".into()),
        phase: "cleaning".into(),
        emitted: vec!["cleanup".into()],
        pending: 2,
    };

    let difference = observation_diff(&[expected], &[actual]);

    let actual_ = difference.contains(field);

    assert!(actual_, "{difference}");
}

#[test]
fn mismatched_observation_counts_report_the_length() {
    let actual = Observation {
        verdict: "accepted".into(),
        error_code: None,
        phase: "running".into(),
        emitted: vec![],
        pending: 0,
    };

    let difference = observation_diff(&[], &[actual]);

    let actual_ = difference.contains("length");

    assert!(actual_);
}

#[rstest::rstest]
#[case(|case: &mut OracleCase| case.scenario = "unknown".into(), CaseClass::InfrastructureError, Some("unknown scenario"))]
#[case(|case: &mut OracleCase| case.schedule = vec!["unknown-event".into()], CaseClass::InfrastructureError, None)]
#[case(|case: &mut OracleCase| case.expected[0].pending += 1, CaseClass::Mismatch, Some("pending"))]
fn oracle_execution_distinguishes_setup_event_and_semantic_failures(
    #[case] change: fn(&mut OracleCase),
    #[case] class: CaseClass,
    #[case] expected_detail: Option<&str>,
) {
    let mut case = load_corpus().unwrap().remove(0);
    change(&mut case);

    let result = run_case(&case);

    let actual = result.detail.is_some();
    let actual_2 =
        expected_detail.is_none_or(|detail| result.detail.as_ref().unwrap().contains(detail));

    assert_eq!(result.class, class);
    assert!(actual);
    assert!(actual_2);
}

#[rstest::rstest]
#[case(&"owned".to_owned(), "owned")]
#[case(&123_u32, "non-string panic payload")]
fn panic_diagnostics_accept_owned_and_non_string_payloads(
    #[case] payload: &(dyn std::any::Any + Send),
    #[case] expected: &str,
) {
    let message = panic_message(payload);

    assert_eq!(message, expected);
}

#[rstest::rstest]
#[case(Driver::advance_to_restoration, "setup did not emit RunBaseline")]
#[case(Driver::advance_to_pre_final_cleanup, "setup did not emit AnalyzeFile")]
#[case(Driver::complete_final_output, "no final output effect is pending")]
fn oracle_setup_rejects_missing_effects(
    #[case] advance: fn(&mut Driver) -> Result<(), String>,
    #[case] expected: &str,
) {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.outstanding.clear();

    let actual = advance(&mut driver).unwrap_err();

    assert_eq!(actual, expected);
}

#[test]
fn oracle_setup_rejects_a_consumed_state() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.state = None;
    let id = driver.ordinary_id;

    let actual = driver
        .apply_setup_completion(
            RunEvent::TargetsResolved(TargetsResolved {
                id,
                targets: vec![],
            }),
            id,
        )
        .unwrap_err();

    assert_eq!(actual, "setup state is unavailable");
}

#[test]
fn oracle_setup_reports_transition_rejection() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    let id = EffectId(u64::MAX);

    let actual = driver
        .apply_setup_completion(
            RunEvent::TargetsResolved(TargetsResolved {
                id,
                targets: vec![],
            }),
            id,
        )
        .unwrap_err()
        .contains("setup transition failed");

    assert!(actual);
}

#[test]
fn oracle_cannot_continue_after_a_rejected_transition_consumes_state() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.state = None;

    let actual = driver.apply("cancel").unwrap_err();

    assert_eq!(actual, "cannot execute cancel after a rejected transition");
}

#[test]
fn oracle_cleanup_completion_requires_a_pending_cleanup() {
    let driver = Driver::for_scenario("pending_resolve").unwrap();

    let actual = driver.completion_event("complete_cleanup").unwrap_err();

    assert_eq!(actual, "no cleanup effect is pending");
}

#[test]
fn oracle_setup_ignores_unrelated_outstanding_effects() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver
        .outstanding
        .push(RunEffect::Cleanup(hoimin_core::Cleanup {
            id: EffectId(999),
            reservations: vec![],
        }));
    driver.advance_to_normal_cleanup().unwrap();

    let actual = driver.state.as_ref().unwrap().phase();

    assert_eq!(actual, RunPhase::Cleaning);
}

#[test]
fn oracle_parser_reports_invalid_json_at_its_line() {
    let actual = parse_corpus("{")
        .unwrap_err()
        .contains("line 1 is invalid JSON");

    assert!(actual);
}

#[test]
fn oracle_final_output_requires_an_owned_state() {
    let mut driver = Driver::for_scenario("final_pending_without_copy").unwrap();
    driver.state = None;

    let actual = driver.complete_final_output().unwrap_err();

    assert_eq!(actual, "final output state is unavailable");
}

#[test]
fn oracle_final_output_reports_acknowledgement_rejection() {
    let mut driver = Driver::for_scenario("final_pending_without_copy").unwrap();
    driver.state = Driver::for_scenario("pending_resolve").unwrap().state;

    let actual = driver
        .complete_final_output()
        .unwrap_err()
        .contains("cannot acknowledge RunFinished");

    assert!(actual);
}

#[test]
fn oracle_baseline_setup_propagates_a_consumed_state_error() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.state = None;

    let actual = driver.advance_to_baseline().unwrap_err();

    assert_eq!(actual, "setup state is unavailable");
}

#[test]
fn oracle_empty_analysis_setup_propagates_missing_baseline_error() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();

    let actual = driver.advance_to_empty_analysis().unwrap_err();

    assert_eq!(actual, "setup did not emit RunBaseline");
}

#[test]
fn oracle_cleanup_setup_propagates_a_consumed_state_error() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.state = None;

    let actual = driver.advance_to_normal_cleanup().unwrap_err();

    assert_eq!(actual, "setup state is unavailable");
}

#[test]
fn missing_integrity_verification_has_a_specific_setup_diagnostic() {
    let actual = pending_verification(&[]).unwrap_err();

    assert_eq!(actual, "setup did not emit VerifyOriginals");
}

#[test]
fn restoration_effect_projects_to_the_model_restoration_label() {
    let effect = RunEffect::ResetWorker(hoimin_core::ResetWorker {
        id: EffectId(1),
        worker: 0,
    });

    let actual = normalize_effect(&effect);

    assert_eq!(actual.as_deref(), Some("restoration"));
}

#[test]
fn equal_observations_have_no_reported_differences() {
    let expected = Observation {
        verdict: "accepted".into(),
        error_code: None,
        phase: "running".into(),
        emitted: vec![],
        pending: 0,
    };
    let actual = Observation {
        verdict: expected.verdict.clone(),
        error_code: expected.error_code.clone(),
        phase: expected.phase.clone(),
        emitted: expected.emitted.clone(),
        pending: expected.pending,
    };

    let actual_ = observation_diff(&[expected], &[actual]);

    assert_eq!(actual_, "");
}

#[rstest::rstest]
#[case(false, CaseClass::Match, None)]
#[case(
    true,
    CaseClass::InfrastructureError,
    Some("case panicked: isolation probe")
)]
fn isolation_boundary_preserves_success_or_classifies_callback_panic(
    #[case] should_panic: bool,
    #[case] class: CaseClass,
    #[case] detail: Option<&str>,
) {
    let case = load_corpus().unwrap().remove(0);

    let result = isolate_case(&case, || {
        assert!(!should_panic, "isolation probe");
        run_case(&case)
    });

    let actual = result.detail.as_deref();

    assert_eq!(result.class, class);
    assert_eq!(actual, detail);
}

#[test]
fn final_output_lookup_ignores_an_unrelated_outstanding_effect() {
    let mut driver = Driver::for_scenario("final_pending_without_copy").unwrap();
    driver.outstanding.insert(
        0,
        RunEffect::ResolveTargets(hoimin_core::ResolveTargets {
            id: EffectId(999),
            selection: hoimin_core::Selection::default(),
        }),
    );
    driver.complete_final_output().unwrap();

    let actual = driver.state.as_ref().unwrap().phase();

    assert_eq!(actual, RunPhase::Finished);
}
