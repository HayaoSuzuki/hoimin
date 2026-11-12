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
            _ => unreachable!(),
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
