use super::*;

#[test]
fn parser_and_validator_report_invalid_contracts() {
    assert!(parse_corpus("{").unwrap_err().contains("invalid JSON"));
    assert!(parse_corpus("[]").unwrap_err().contains("not an object"));
    assert!(parse_corpus("").unwrap_err().contains("no cases"));
    let row = CORPUS.lines().find(|line| !line.is_empty()).unwrap();
    assert!(
        parse_corpus(row)
            .unwrap_err()
            .contains("exact schema-owned")
    );
    assert_eq!(
        parse_corpus(&format!("\n{CORPUS}")).unwrap().len(),
        EXPECTED_CASE_KEYS.len()
    );
    let mut value: serde_json::Value = serde_json::from_str(row).unwrap();
    value["schema"] = serde_json::json!("wrong");
    assert!(
        parse_corpus(&value.to_string())
            .unwrap_err()
            .contains("violates the schema")
    );
    assert!(!facts_are_normalized(&[
        "direct:Sequence=typing.Sequence".into(),
        "direct:Sequence=typing.Sequence".into()
    ]));
    let mut case = parse_corpus(CORPUS).unwrap().remove(0);
    case.id.clear();
    assert!(
        validate_case(&case)
            .unwrap_err()
            .contains("invalid identity")
    );
    case = parse_corpus(CORPUS).unwrap().remove(0);
    case.observation_kind = "unknown".into();
    assert!(!fields_match(&case));
    case = parse_corpus(CORPUS).unwrap().remove(0);
    case.expected_facts = vec!["unknown".into()];
    assert!(
        validate_case(&case)
            .unwrap_err()
            .contains("invalid normalized facts")
    );
}

#[test]
fn fixture_failures_preserve_stage_and_case_identity() {
    let item = parse_corpus(CORPUS).unwrap().remove(0);
    assert!(
        Fixture::from_tempdir(&item, Err(std::io::Error::other("denied")))
            .err()
            .unwrap()
            .contains("fixture tempdir: denied")
    );
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("src/case.py")).unwrap();
    assert!(
        write_fixture_files(&item, directory.path())
            .unwrap_err()
            .contains("fixture write source")
    );
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("pyproject.toml")).unwrap();
    assert!(
        write_fixture_files(&item, directory.path())
            .unwrap_err()
            .contains("fixture write project metadata")
    );
}

#[test]
fn wait_and_exit_status_decode_all_platform_states() {
    assert!(
        decode_wait("case", Ok(Err(std::io::Error::other("reaped"))))
            .unwrap_err()
            .contains("wait failed: reaped")
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert!(matches!(
            process_termination(ExitStatus::from_raw(0)),
            ProcessTermination::Success
        ));
        assert!(matches!(
            process_termination(ExitStatus::from_raw(256)),
            ProcessTermination::Exit(1)
        ));
        assert!(matches!(
            process_termination(ExitStatus::from_raw(9)),
            ProcessTermination::Signal { signal: 9, .. }
        ));
        assert!(matches!(
            process_termination(ExitStatus::from_raw(127)),
            ProcessTermination::Abnormal
        ));
    }
}

#[tokio::test]
async fn normalization_and_operator_presence_reject_invalid_inputs() {
    let mut item = parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .find(|item| item.mode == "strict" && item.expected_present)
        .unwrap();
    let manifest = public_plan(&item).await.unwrap();
    let original_marker = item.marker.clone();
    item.marker =
        item.source[..usize::try_from(manifest.candidates[0].span.start).unwrap()].to_owned();
    assert_eq!(normalize_manifest(&item, &manifest).unwrap().count, 0);
    item.marker = original_marker;
    let mut changed = manifest.clone();
    changed.candidates[0].candidate.span.start = u64::MAX;
    changed.candidates[0].candidate.span.length = 1;
    assert!(
        normalize_manifest(&item, &changed)
            .unwrap_err()
            .contains("candidate range overflow")
    );
    changed = manifest.clone();
    changed.candidates[0].candidate.original = "wrong".into();
    assert!(
        normalize_manifest(&item, &changed)
            .unwrap_err()
            .contains("span does not match")
    );
    item.marker = "missing".into();
    assert!(
        normalize_manifest(&item, &manifest)
            .unwrap_err()
            .contains("marker occurs 0 times")
    );
    item.operator = None;
    assert!(
        public_plan(&item)
            .await
            .unwrap_err()
            .contains("operator missing")
    );
}
