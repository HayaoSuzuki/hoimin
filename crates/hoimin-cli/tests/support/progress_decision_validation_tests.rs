use super::*;

fn first_case() -> OracleCase {
    parse_corpus().unwrap().remove(0)
}

#[test]
fn parser_rejects_invalid_json() {
    let actual_0 = parse_corpus_from("{");

    assert!(actual_0.unwrap_err().contains("invalid JSON"));
}

#[test]
fn parser_rejects_empty_corpus() {
    let actual_0 = parse_corpus_from("");

    assert!(actual_0.unwrap_err().contains("no cases"));
}

#[test]
fn parser_rejects_duplicate_identity() {
    let row = CORPUS.lines().find(|row| !row.is_empty()).unwrap();

    let actual_0 = parse_corpus_from(&format!("{row}\n{row}"));

    assert!(actual_0.unwrap_err().contains("duplicate case id"));
}

#[test]
fn parser_ignores_empty_lines() {
    let actual_0 = parse_corpus_from(&format!("\n{CORPUS}"));

    assert_eq!(actual_0.unwrap().len(), 26);
}

#[rstest::rstest]
#[case::case_0("schema", json!(2), "invalid schema")]
#[case::case_1("id", json!(""), "invalid schema")]
#[case::case_2("patience", json!(0), "invalid schema")]
#[case::case_3("mode", json!("unknown"), "unknown mode")]
#[case::case_4("reports", json!([]), "no reports")]
fn validator_rejects_invalid_corpus_fields(
    #[case] field: &str,
    #[case] value: Value,
    #[case] expected: &str,
) {
    let mut row: Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    row[field] = value;
    let case: OracleCase = serde_json::from_value(row).unwrap();

    let actual_0 = validate_case(&case);

    assert!(actual_0.unwrap_err().contains(expected));
}

#[test]
fn validator_rejects_unknown_state() {
    let mut case = first_case();
    case.expected.latest_state = "unknown".into();

    let actual_0 = validate_case(&case);

    assert!(actual_0.unwrap_err().contains("invalid state"));
}

#[test]
fn validator_rejects_inconsistent_saturation() {
    let mut case = first_case();
    case.expected.saturated = !case.expected.saturated;

    let actual_0 = validate_case(&case);

    assert!(actual_0.unwrap_err().contains("inconsistent saturation"));
}

#[test]
fn validator_rejects_unknown_reason() {
    let mut case = first_case();
    case.reports[0].reason = Some("unknown".into());

    let actual_0 = validate_case(&case);

    assert!(actual_0.unwrap_err().contains("inconsistent report reason"));
}

#[rstest::rstest]
#[case::case_0(2, 0, "killed", "role outside")]
#[case::case_1(0, 2, "killed", "role outside")]
#[case::case_2(0, 0, "unknown", "unknown status")]
fn validator_rejects_invalid_mutant(
    #[case] candidate_id: u64,
    #[case] content_key: u64,
    #[case] status: &str,
    #[case] expected: &str,
) {
    let mut case = first_case();
    case.reports[0].mutants = vec![OracleMutant {
        candidate_id,
        content_key,
        status: status.into(),
    }];

    let actual_0 = validate_case(&case);

    assert!(actual_0.unwrap_err().contains(expected));
}

#[test]
fn validator_rejects_repeated_candidate() {
    let mut case = first_case();
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

    let actual_0 = validate_case(&case);

    assert!(actual_0.unwrap_err().contains("repeats a candidate"));
}

#[test]
fn validator_rejects_unknown_eligibility() {
    let mut cases = parse_corpus().unwrap();
    let case = cases
        .iter_mut()
        .find(|case| !case.expected.comparisons.is_empty())
        .unwrap();
    case.expected.comparisons[0].eligibility = "unknown".into();

    let actual_0 = validate_case(case);

    assert!(actual_0.unwrap_err().contains("unknown eligibility"));
}

#[rstest::rstest]
#[case::case_0(&ExpectedScore {
            killed: 0,
            decidable: 0,
        })]
#[case::case_1(&ExpectedScore {
            killed: 2,
            decidable: 1,
        })]
#[case::case_2(&ExpectedScore {
            killed: 1,
            decidable: 3,
        })]
fn score_rejects_unsupported_fraction(#[case] score: &ExpectedScore) {
    let result = validate_score("case", Some(score));

    assert!(result.is_err());
}

#[rstest::rstest]
#[case::case_0(&ExpectedDelta {
            numerator: 1,
            denominator: 0,
        })]
#[case::case_1(&ExpectedDelta {
            numerator: 1,
            denominator: 3,
        })]
#[case::case_2(&ExpectedDelta {
            numerator: 1,
            denominator: u64::MAX,
        })]
fn delta_rejects_unsupported_fraction(#[case] delta: &ExpectedDelta) {
    let result = validate_delta("case", Some(delta));

    assert!(result.is_err());
}

#[test]
fn exact_score_rejects_inexact_fraction() {
    let actual_0 = exact_score(1, 3);

    assert_eq!(actual_0, None);
}

#[test]
fn exact_delta_preserves_negative_half() {
    let actual_0 = exact_delta(&ExpectedDelta {
        numerator: -1,
        denominator: 2,
    });

    assert_eq!(actual_0, Some(-0.5));
}

#[tokio::test(start_paused = true)]
async fn public_execution_reports_tempdir_failure() {
    let case = first_case();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_hoimin"));

    let actual_0 = public_output_from(
        &case,
        false,
        false,
        Err(std::io::Error::other("denied")),
        executable,
    )
    .await;

    assert_eq!(actual_0.unwrap_err(), "denied");
}

#[tokio::test(start_paused = true)]
async fn public_execution_reports_write_failure() {
    let case = first_case();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_hoimin"));
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join("malformed.json")).unwrap();

    let actual_0 = public_output_from(&case, false, true, Ok(fixture), executable).await;

    assert!(actual_0.is_err());
}

#[tokio::test(start_paused = true)]
async fn public_execution_reports_spawn_failure() {
    let case = first_case();

    let actual_0 = public_output_from(
        &case,
        false,
        false,
        tempfile::tempdir(),
        std::path::Path::new("/missing-hoimin-command"),
    )
    .await;

    assert!(actual_0.unwrap_err().contains("spawn failed"));
}

#[tokio::test(start_paused = true)]
async fn public_execution_reports_timeout() {
    let case = first_case();
    let elapsed = tokio::time::timeout(
        Duration::from_millis(1),
        std::future::pending::<std::io::Result<std::process::Output>>(),
    )
    .await;

    let actual_0 = decode_output(&case, elapsed);

    assert!(actual_0.unwrap_err().contains("CLI timed out"));
}

#[cfg(unix)]
#[rstest::rstest]
#[case::exit(256, b"".as_slice(), b"failed".as_slice(), "stderr=failed")]
#[case::json(0, b"{".as_slice(), b"".as_slice(), "invalid JSON")]
fn decoder_reports_invalid_output(
    #[case] status: i32,
    #[case] stdout: &[u8],
    #[case] stderr: &[u8],
    #[case] expected: &str,
) {
    use std::os::unix::process::ExitStatusExt;
    let case = first_case();
    let output = std::process::Output {
        status: std::process::ExitStatus::from_raw(status),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    };

    let result = decode_progress(&case, &output);

    assert!(result.unwrap_err().contains(expected));
}

#[test]
fn summary_rejects_unknown_status() {
    let invalid = OracleMutant {
        candidate_id: 0,
        content_key: 0,
        status: "unknown".into(),
    };

    let actual_0 = std::panic::catch_unwind(|| summary_counts(&[invalid]));

    assert!(actual_0.is_err());
}

#[test]
fn report_rejects_unknown_reason() {
    let invalid = OracleReport {
        usable: false,
        reason: Some("unknown".into()),
        mutants: vec![],
    };

    let actual_0 = std::panic::catch_unwind(|| build_report("case", 0, &invalid));

    assert!(actual_0.is_err());
}

#[test]
fn candidate_identity_rejects_unknown_role() {
    let actual_0 = std::panic::catch_unwind(|| expected_public_candidate_id(2, 0));

    assert!(actual_0.is_err());
}

#[test]
fn injected_report_rejects_unusable_input() {
    let mut case = first_case();
    case.reports[0].usable = false;

    let actual_0 =
        std::panic::catch_unwind(|| inject_usable_reports_without_reader_validation(&case));

    assert!(actual_0.is_err());
}

#[test]
fn comparison_assertion_rejects_non_array_payload() {
    let item = first_case();
    let actual = json!({"patience":item.patience,"consecutive_stalls":item.expected.consecutive_stalls,"latest":{"state":item.expected.latest_state,"consecutive_stalls":item.expected.consecutive_stalls,"patience":item.patience,"saturated":item.expected.saturated},"comparisons":null});

    let actual_0 = std::panic::catch_unwind(|| assert_case_matches(&item, &actual));

    assert!(actual_0.is_err());
}

#[rstest::rstest]
fn adapter_vectors_cover_candidate_roles(
    #[values(0u64, 1u64)] candidate_id: u64,
    #[values(0u64, 1u64)] content_key: u64,
) {
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

#[test]
fn exact_delta_preserves_positive_half() {
    let actual_0 = exact_delta(&ExpectedDelta {
        numerator: 1,
        denominator: 2,
    });

    assert_eq!(actual_0, Some(0.5));
}

#[rstest::rstest]
#[case::case_0(ProgressState::Improving, "improving")]
#[case::case_1(ProgressState::Regressing, "regressing")]
#[case::case_2(ProgressState::Stalled, "stalled")]
#[case::case_3(ProgressState::Saturated, "saturated")]
#[case::case_4(ProgressState::Indeterminate, "indeterminate")]
fn state_names_cover_progress_states(#[case] state: ProgressState, #[case] expected: &str) {
    let actual_0 = state_name(state);

    assert_eq!(actual_0, expected);
}
