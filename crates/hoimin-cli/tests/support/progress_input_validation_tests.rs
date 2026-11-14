use super::*;

#[test]
fn classifier_preserves_mismatch_details() {
    let case = parse_lean_oracle_cases().remove(0);
    let mismatch = Ok(CliObservation {
        disposition: "unexpected".into(),
        history: None,
    });

    let (class, detail) = classify_observation(&case, &mismatch);

    assert_eq!(class, "mismatch");
    assert!(detail.unwrap().contains("observed"));
}

#[test]
fn classifier_preserves_infrastructure_details() {
    let case = parse_lean_oracle_cases().remove(0);

    let actual_0 = classify_observation(&case, &Err("unavailable".into()));

    assert_eq!(
        actual_0,
        ("infrastructure-error", Some("unavailable".into()))
    );
}

#[rstest::rstest]
#[case::case_0(
            2,
            b"unexpected".as_slice(),
            "invalid structure in report fixture.report",
            "diagnostic/output contract",
        )]
#[case::case_1(
            2,
            b"".as_slice(),
            "could not parse report elsewhere",
            "diagnostic/output contract",
        )]
#[case::case_2(1, b"".as_slice(), "failure", "CLI failed with Some(1)")]
#[case::case_3(0, b"{".as_slice(), "", "CLI JSON")]
#[cfg(unix)]
fn decoder_rejects_invalid_process_output(
    #[case] code: i32,
    #[case] stdout: &[u8],
    #[case] stderr: &str,
    #[case] expected: &str,
) {
    use std::os::unix::process::ExitStatusExt;
    let path = std::path::Path::new("fixture.report");
    let output = std::process::Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    };

    let actual_0 = decode_cli_output(path, &output);

    assert!(actual_0.unwrap_err().contains(expected));
}

#[rstest::rstest]
#[case::case_0(&json!({}), "missing inputs")]
#[case::case_1(&json!({"inputs":[]}), "inconsistent CLI input")]
#[case::case_2(
            &json!({"inputs":[{"usable":true},{"usable":false}]}),
            "inconsistent CLI input",
        )]
#[case::case_3(&json!({"inputs":[{},{}]}), "missing usable")]
#[case::case_4(
            &json!({"inputs":[{"usable":false},{"usable":false}]}),
            "missing unusable reason",
        )]
#[case::case_5(
            &json!({"inputs":[{"usable":true},{"usable":true}]}),
            "missing state",
        )]
#[case::case_6(
            &json!({"inputs":[{"usable":true},{"usable":true}],"latest":{"state":"stalled"}}),
            "missing stalls",
        )]
#[case::case_7(
            &json!({"inputs":[{"usable":true},{"usable":true}],"latest":{"state":"stalled"},"consecutive_stalls":0}),
            "missing comparisons",
        )]
#[cfg(unix)]
fn decoder_rejects_invalid_progress_document(#[case] payload: &Value, #[case] expected: &str) {
    use std::os::unix::process::ExitStatusExt;

    let path = std::path::Path::new("fixture.report");

    let mut stdout = serde_json::to_vec(payload).unwrap();
    stdout.push(b'\n');
    let output = std::process::Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout,
        stderr: vec![],
    };

    let result = decode_cli_output(path, &output);

    assert!(result.unwrap_err().contains(expected));
}

#[tokio::test(start_paused = true)]
async fn execution_reports_tempdir_failure() {
    let case = parse_lean_oracle_cases().remove(0);
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_hoimin"));

    let actual_0 =
        observe_cli_encoding_from(&case, Err(std::io::Error::other("denied")), executable).await;

    assert_eq!(actual_0.unwrap_err(), "denied");
}

#[tokio::test(start_paused = true)]
async fn execution_reports_report_write_failure() {
    let case = parse_lean_oracle_cases().remove(0);
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_hoimin"));
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join(format!("{}.report", case.id))).unwrap();

    let actual_0 = observe_cli_encoding_from(&case, Ok(fixture), executable).await;

    assert!(actual_0.is_err());
}

#[tokio::test(start_paused = true)]
async fn execution_reports_spawn_failure() {
    let case = parse_lean_oracle_cases().remove(0);

    let actual_0 = observe_cli_encoding_from(
        &case,
        tempfile::tempdir(),
        std::path::Path::new("/missing-cli"),
    )
    .await;

    assert!(actual_0.is_err());
}

#[tokio::test(start_paused = true)]
async fn decoder_reports_deadline() {
    let elapsed = tokio::time::timeout(
        Duration::from_millis(1),
        std::future::pending::<std::io::Result<std::process::Output>>(),
    )
    .await;

    let actual_0 = decode_cli_wait(elapsed);

    assert_eq!(actual_0.unwrap_err(), "CLI timeout");
}

#[test]
fn boundary_selector_accepts_matching_identity() {
    let case = parse_lean_oracle_cases().remove(0);

    let actual_0 = selected_boundary_case(&case, Some(&case.id));

    assert!(actual_0);
}

#[test]
fn boundary_selector_rejects_unknown_identity() {
    let case = parse_lean_oracle_cases().remove(0);

    let actual_0 = selected_boundary_case(&case, Some("unknown"));

    assert!(!actual_0);
}

#[test]
fn unexecuted_projection_reports_mismatch() {
    let mut case = parse_lean_oracle_cases().remove(0);
    case.baseline = "missing".into();
    case.statuses = vec!["killed".into()];
    print_unexecuted_gap(&case);

    let actual_0 = failure_message(&case, "jsonl-v6", "mismatch", Some("different"));

    assert!(actual_0.unwrap().contains("different"));
}

#[test]
fn matching_report_has_no_failure() {
    let case = parse_lean_oracle_cases().remove(0);

    let actual_0 = failure_message(&case, "jsonl-v6", "match", None);

    assert!(actual_0.is_none());
}

#[test]
fn report_rejects_unknown_baseline() {
    let mut case = parse_lean_oracle_cases().remove(0);
    case.baseline = "unknown".into();

    let actual_0 = std::panic::catch_unwind(|| report_json_from_lean_case(&case));

    assert!(actual_0.is_err());
}
