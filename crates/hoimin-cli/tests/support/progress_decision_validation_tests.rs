use super::*;

fn first_case() -> OracleCase {
    parse_corpus().unwrap().remove(0)
}

#[test]
fn parser_rejects_invalid_json_empty_and_duplicate_identities() {
    assert!(parse_corpus_from("{").unwrap_err().contains("invalid JSON"));
    assert!(parse_corpus_from("").unwrap_err().contains("no cases"));
    let row = CORPUS.lines().find(|row| !row.is_empty()).unwrap();
    assert!(
        parse_corpus_from(&format!("{row}\n{row}"))
            .unwrap_err()
            .contains("duplicate case id")
    );
    assert_eq!(parse_corpus_from(&format!("\n{CORPUS}")).unwrap().len(), 26);
}

#[test]
fn validator_rejects_each_invalid_contract() {
    for (field, value, expected) in [
        ("schema", json!(2), "invalid schema"),
        ("id", json!(""), "invalid schema"),
        ("patience", json!(0), "invalid schema"),
        ("mode", json!("unknown"), "unknown mode"),
        ("reports", json!([]), "no reports"),
    ] {
        let mut row: Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
        row[field] = value;
        let case: OracleCase = serde_json::from_value(row).unwrap();
        assert!(validate_case(&case).unwrap_err().contains(expected));
    }
    let mut case = first_case();
    case.expected.latest_state = "unknown".into();
    assert!(validate_case(&case).unwrap_err().contains("invalid state"));
    case = first_case();
    case.expected.saturated = !case.expected.saturated;
    assert!(
        validate_case(&case)
            .unwrap_err()
            .contains("inconsistent saturation")
    );
    case = first_case();
    case.reports[0].reason = Some("unknown".into());
    assert!(
        validate_case(&case)
            .unwrap_err()
            .contains("inconsistent report reason")
    );
    for (candidate_id, content_key, status, expected) in [
        (2, 0, "killed", "role outside"),
        (0, 2, "killed", "role outside"),
        (0, 0, "unknown", "unknown status"),
    ] {
        case = first_case();
        case.reports[0].mutants = vec![OracleMutant {
            candidate_id,
            content_key,
            status: status.into(),
        }];
        assert!(validate_case(&case).unwrap_err().contains(expected));
    }
    case = first_case();
    case.mode = "strict".into();
    case.reports[0].mutants = vec![
        OracleMutant {
            candidate_id: 0,
            content_key: 0,
            status: "killed".into(),
        },
        OracleMutant {
            candidate_id: 0,
            content_key: 1,
            status: "killed".into(),
        },
    ];
    assert!(
        validate_case(&case)
            .unwrap_err()
            .contains("repeats a candidate")
    );
    let mut cases = parse_corpus().unwrap();
    let case = cases
        .iter_mut()
        .find(|case| !case.expected.comparisons.is_empty())
        .unwrap();
    case.expected.comparisons[0].eligibility = "unknown".into();
    assert!(
        validate_case(case)
            .unwrap_err()
            .contains("unknown eligibility")
    );
}

#[test]
fn exact_fraction_validation_rejects_unsupported_boundaries() {
    for score in [
        ExpectedScore {
            killed: 0,
            decidable: 0,
        },
        ExpectedScore {
            killed: 2,
            decidable: 1,
        },
        ExpectedScore {
            killed: 1,
            decidable: 3,
        },
    ] {
        assert!(validate_score("case", Some(&score)).is_err());
    }
    for delta in [
        ExpectedDelta {
            numerator: 1,
            denominator: 0,
        },
        ExpectedDelta {
            numerator: 1,
            denominator: 3,
        },
        ExpectedDelta {
            numerator: 1,
            denominator: u64::MAX,
        },
    ] {
        assert!(validate_delta("case", Some(&delta)).is_err());
    }
    assert_eq!(exact_score(1, 3), None);
    assert_eq!(
        exact_delta(&ExpectedDelta {
            numerator: -1,
            denominator: 2
        }),
        Some(-0.5)
    );
}

#[tokio::test(start_paused = true)]
async fn public_execution_classifies_io_timeout_and_decode_failures() {
    let case = first_case();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_hoimin"));
    assert_eq!(
        public_output_from(
            &case,
            false,
            false,
            Err(std::io::Error::other("denied")),
            executable
        )
        .await
        .unwrap_err(),
        "denied"
    );
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join("malformed.json")).unwrap();
    assert!(
        public_output_from(&case, false, true, Ok(fixture), executable)
            .await
            .is_err()
    );
    assert!(
        public_output_from(
            &case,
            false,
            false,
            tempfile::tempdir(),
            std::path::Path::new("/missing-hoimin-command")
        )
        .await
        .unwrap_err()
        .contains("spawn failed")
    );
    let elapsed = tokio::time::timeout(
        Duration::from_millis(1),
        std::future::pending::<std::io::Result<std::process::Output>>(),
    )
    .await;
    assert!(
        decode_output(&case, elapsed)
            .unwrap_err()
            .contains("CLI timed out")
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout: vec![],
            stderr: b"failed".to_vec(),
        };
        assert!(
            decode_progress(&case, &output)
                .unwrap_err()
                .contains("stderr=failed")
        );
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"{".to_vec(),
            stderr: vec![],
        };
        assert!(
            decode_progress(&case, &output)
                .unwrap_err()
                .contains("invalid JSON")
        );
    }
}

#[test]
fn invalid_report_inputs_fail_at_the_expected_boundary() {
    let invalid = OracleMutant {
        candidate_id: 0,
        content_key: 0,
        status: "unknown".into(),
    };
    assert!(std::panic::catch_unwind(|| summary_counts(&[invalid])).is_err());
    let invalid = OracleReport {
        usable: false,
        reason: Some("unknown".into()),
        mutants: vec![],
    };
    assert!(std::panic::catch_unwind(|| build_report("case", 0, &invalid)).is_err());
    assert!(std::panic::catch_unwind(|| expected_public_candidate_id(2, 0)).is_err());
    let mut case = first_case();
    case.reports[0].usable = false;
    assert!(
        std::panic::catch_unwind(|| inject_usable_reports_without_reader_validation(&case))
            .is_err()
    );
}

#[test]
fn comparison_assertion_rejects_non_array_payload() {
    let item = first_case();
    let actual = json!({"patience":item.patience,"consecutive_stalls":item.expected.consecutive_stalls,"latest":{"state":item.expected.latest_state,"consecutive_stalls":item.expected.consecutive_stalls,"patience":item.patience,"saturated":item.expected.saturated},"comparisons":null});
    assert!(std::panic::catch_unwind(|| assert_case_matches(&item, &actual)).is_err());
}

#[test]
fn adapter_vectors_cover_all_candidate_roles_exact_deltas_and_states() {
    for candidate_id in 0..=1 {
        for content_key in 0..=1 {
            let report = OracleReport {
                usable: true,
                reason: None,
                mutants: vec![OracleMutant {
                    candidate_id,
                    content_key,
                    status: "killed".into(),
                }],
            };
            let mut document = build_report("vectors", 0, &report);
            let candidate = &mut document["mutants"][0]["candidate"];
            candidate["file_hash"] = json!(format!("hash-role-{candidate_id}"));
            candidate_identity::assign_id(candidate);
            assert_eq!(
                candidate["id"],
                expected_public_candidate_id(candidate_id, content_key)
            );
        }
    }
    assert_eq!(
        exact_delta(&ExpectedDelta {
            numerator: 1,
            denominator: 2
        }),
        Some(0.5)
    );
    for (state, expected) in [
        (ProgressState::Improving, "improving"),
        (ProgressState::Regressing, "regressing"),
        (ProgressState::Stalled, "stalled"),
        (ProgressState::Saturated, "saturated"),
        (ProgressState::Indeterminate, "indeterminate"),
    ] {
        assert_eq!(state_name(state), expected);
    }
}
