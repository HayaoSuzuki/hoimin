use super::*;

#[test]
fn classifier_preserves_mismatch_and_infrastructure_details() {
    let case = parse_lean_oracle_cases().remove(0);
    let mismatch = Ok(CliObservation {
        disposition: "unexpected".into(),
        history: None,
    });
    let (class, detail) = classify_observation(&case, &mismatch);
    assert_eq!(class, "mismatch");
    assert!(detail.unwrap().contains("observed"));
    assert_eq!(
        classify_observation(&case, &Err("unavailable".into())),
        ("infrastructure-error", Some("unavailable".into()))
    );
}

#[cfg(unix)]
#[test]
fn decoder_rejects_invalid_cli_output_contracts() {
    use std::os::unix::process::ExitStatusExt;
    let path = std::path::Path::new("fixture.report");
    for (code, stdout, stderr, expected) in [
        (
            2,
            b"unexpected".as_slice(),
            "invalid structure in report fixture.report",
            "diagnostic/output contract",
        ),
        (
            2,
            b"".as_slice(),
            "could not parse report elsewhere",
            "diagnostic/output contract",
        ),
        (1, b"".as_slice(), "failure", "CLI failed with Some(1)"),
        (0, b"{".as_slice(), "", "CLI JSON"),
    ] {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        };
        assert!(
            decode_cli_output(path, &output)
                .unwrap_err()
                .contains(expected)
        );
    }
    for (payload, expected) in [
        (json!({}), "missing inputs"),
        (json!({"inputs":[]}), "inconsistent CLI input"),
        (
            json!({"inputs":[{"usable":true},{"usable":false}]}),
            "inconsistent CLI input",
        ),
        (json!({"inputs":[{},{}]}), "missing usable"),
        (
            json!({"inputs":[{"usable":false},{"usable":false}]}),
            "missing unusable reason",
        ),
        (
            json!({"inputs":[{"usable":true},{"usable":true}]}),
            "missing state",
        ),
        (
            json!({"inputs":[{"usable":true},{"usable":true}],"latest":{"state":"stalled"}}),
            "missing stalls",
        ),
        (
            json!({"inputs":[{"usable":true},{"usable":true}],"latest":{"state":"stalled"},"consecutive_stalls":0}),
            "missing comparisons",
        ),
    ] {
        let mut stdout = serde_json::to_vec(&payload).unwrap();
        stdout.push(b'\n');
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout,
            stderr: vec![],
        };
        assert!(
            decode_cli_output(path, &output)
                .unwrap_err()
                .contains(expected)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn execution_reports_fixture_io_spawn_and_deadline_errors() {
    let case = parse_lean_oracle_cases().remove(0);
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_hoimin"));
    assert_eq!(
        observe_cli_encoding_from(&case, Err(std::io::Error::other("denied")), executable)
            .await
            .unwrap_err(),
        "denied"
    );
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join(format!("{}.report", case.id))).unwrap();
    assert!(
        observe_cli_encoding_from(&case, Ok(fixture), executable)
            .await
            .is_err()
    );
    assert!(
        observe_cli_encoding_from(
            &case,
            tempfile::tempdir(),
            std::path::Path::new("/missing-cli")
        )
        .await
        .is_err()
    );
    let elapsed = tokio::time::timeout(
        Duration::from_millis(1),
        std::future::pending::<std::io::Result<std::process::Output>>(),
    )
    .await;
    assert_eq!(decode_cli_wait(elapsed).unwrap_err(), "CLI timeout");
}

#[test]
fn boundary_reporting_handles_explicit_selection_and_unexecuted_projection() {
    let mut case = parse_lean_oracle_cases().remove(0);
    assert!(selected_boundary_case(&case, Some(&case.id)));
    assert!(!selected_boundary_case(&case, Some("unknown")));
    case.baseline = "missing".into();
    case.statuses = vec!["killed".into()];
    print_unexecuted_gap(&case);
    assert!(
        failure_message(&case, "jsonl-v6", "mismatch", Some("different"))
            .unwrap()
            .contains("different")
    );
    assert!(failure_message(&case, "jsonl-v6", "match", None).is_none());
    case.baseline = "unknown".into();
    assert!(std::panic::catch_unwind(|| report_json_from_lean_case(&case)).is_err());
}
