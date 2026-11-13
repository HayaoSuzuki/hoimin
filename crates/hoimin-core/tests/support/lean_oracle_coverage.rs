use super::*;

#[test]
fn malformed_oracle_cases_are_rejected_before_execution() {
    for (variant, message) in [
        (0, "no executable steps"),
        (1, "scheduled events"),
        (2, "does not match schedule"),
        (3, "unknown verdict"),
        (4, "unknown error code"),
    ] {
        let mut case = load_corpus().unwrap().remove(0);
        match variant {
            0 => case.schedule.clear(),
            1 => case.schedule.push("cancel".into()),
            2 => case.expected[0].event = "different".into(),
            3 => case.expected[0].verdict = "unknown".into(),
            4 => case.expected[0].error_code = Some("unknown".into()),
            _ => {
                // Test failure reporting only; a passing scenario cannot execute this expression.
                #[cfg(coverage_nightly)]
                {
                    (#[coverage(off)]
                    || -> ! { unreachable!() })()
                }
                #[cfg(not(coverage_nightly))]
                {
                    unreachable!()
                }
            }
        }
        assert!(validate_case(&case).unwrap_err().contains(message));
    }
}

#[test]
fn empty_and_whitespace_separated_corpora_are_handled() {
    assert!(parse_corpus("\n\n").unwrap_err().contains("no cases"));
    let corpus = format!("\n{}\n\n", corpus_text());
    assert_eq!(parse_corpus(&corpus).unwrap().len(), 22);
}

#[test]
fn mismatched_observations_report_every_changed_field() {
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
    for field in ["verdict", "error_code", "phase", "emitted", "pending"] {
        assert!(difference.contains(field), "{difference}");
    }
    assert!(
        observation_diff(
            &[],
            &[Observation {
                verdict: "accepted".into(),
                error_code: None,
                phase: "running".into(),
                emitted: vec![],
                pending: 0,
            }]
        )
        .contains("length")
    );
}

#[test]
fn oracle_execution_distinguishes_setup_event_and_semantic_failures() {
    let mut case = load_corpus().unwrap().remove(0);
    case.scenario = "unknown".into();
    let setup = run_case(&case);
    assert_eq!(setup.class, CaseClass::InfrastructureError);
    assert!(setup.detail.unwrap().contains("unknown scenario"));

    let mut case = load_corpus().unwrap().remove(0);
    case.schedule = vec!["unknown-event".into()];
    let event = run_case(&case);
    assert_eq!(event.class, CaseClass::InfrastructureError);
    assert!(event.detail.is_some());

    let mut case = load_corpus().unwrap().remove(0);
    case.expected[0].pending += 1;
    let mismatch = run_case(&case);
    assert_eq!(mismatch.class, CaseClass::Mismatch);
    assert!(mismatch.detail.unwrap().contains("pending"));
}

#[test]
fn panic_diagnostics_accept_owned_and_non_string_payloads() {
    assert_eq!(panic_message(&"owned".to_owned()), "owned");
    assert_eq!(panic_message(&123_u32), "non-string panic payload");
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
    assert_eq!(advance(&mut driver).unwrap_err(), expected);
}

#[test]
fn oracle_setup_rejects_a_consumed_state() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.state = None;
    let id = driver.ordinary_id;
    assert_eq!(
        driver
            .apply_setup_completion(
                RunEvent::TargetsResolved(TargetsResolved {
                    id,
                    targets: vec![]
                }),
                id
            )
            .unwrap_err(),
        "setup state is unavailable"
    );
}

#[test]
fn oracle_setup_reports_transition_rejection() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    let id = EffectId(u64::MAX);
    assert!(
        driver
            .apply_setup_completion(
                RunEvent::TargetsResolved(TargetsResolved {
                    id,
                    targets: vec![]
                }),
                id
            )
            .unwrap_err()
            .contains("setup transition failed")
    );
}

#[test]
fn oracle_cannot_continue_after_a_rejected_transition_consumes_state() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.state = None;
    assert_eq!(
        driver.apply("cancel").unwrap_err(),
        "cannot execute cancel after a rejected transition"
    );
}

#[test]
fn oracle_cleanup_completion_requires_a_pending_cleanup() {
    let driver = Driver::for_scenario("pending_resolve").unwrap();
    assert_eq!(
        driver.completion_event("complete_cleanup").unwrap_err(),
        "no cleanup effect is pending"
    );
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
    assert_eq!(driver.state.as_ref().unwrap().phase(), RunPhase::Cleaning);
}

#[test]
fn oracle_parser_reports_invalid_json_at_its_line() {
    assert!(
        parse_corpus("{")
            .unwrap_err()
            .contains("line 1 is invalid JSON")
    );
}

#[test]
fn oracle_final_output_requires_an_owned_state() {
    let mut driver = Driver::for_scenario("final_pending_without_copy").unwrap();
    driver.state = None;
    assert_eq!(
        driver.complete_final_output().unwrap_err(),
        "final output state is unavailable"
    );
}

#[test]
fn oracle_final_output_reports_acknowledgement_rejection() {
    let mut driver = Driver::for_scenario("final_pending_without_copy").unwrap();
    driver.state = Driver::for_scenario("pending_resolve").unwrap().state;
    assert!(
        driver
            .complete_final_output()
            .unwrap_err()
            .contains("cannot acknowledge RunFinished")
    );
}

#[test]
fn oracle_baseline_setup_propagates_a_consumed_state_error() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.state = None;
    assert_eq!(
        driver.advance_to_baseline().unwrap_err(),
        "setup state is unavailable"
    );
}

#[test]
fn oracle_empty_analysis_setup_propagates_missing_baseline_error() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    assert_eq!(
        driver.advance_to_empty_analysis().unwrap_err(),
        "setup did not emit RunBaseline"
    );
}

#[test]
fn oracle_cleanup_setup_propagates_a_consumed_state_error() {
    let mut driver = Driver::for_scenario("pending_resolve").unwrap();
    driver.state = None;
    assert_eq!(
        driver.advance_to_normal_cleanup().unwrap_err(),
        "setup state is unavailable"
    );
}

#[test]
fn missing_integrity_verification_has_a_specific_setup_diagnostic() {
    assert_eq!(
        pending_verification(&[]).unwrap_err(),
        "setup did not emit VerifyOriginals"
    );
}

#[test]
fn restoration_effect_projects_to_the_model_restoration_label() {
    let effect = RunEffect::ResetWorker(hoimin_core::ResetWorker {
        id: EffectId(1),
        worker: 0,
    });
    assert_eq!(normalize_effect(&effect).as_deref(), Some("restoration"));
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
    assert_eq!(observation_diff(&[expected], &[actual]), "");
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
    assert_eq!(result.class, class);
    assert_eq!(result.detail.as_deref(), detail);
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
    assert_eq!(driver.state.as_ref().unwrap().phase(), RunPhase::Finished);
}
