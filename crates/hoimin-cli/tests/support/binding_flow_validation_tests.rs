use super::*;

#[test]
fn parser_and_state_contracts_reject_corruption() {
    assert!(parse_corpus("{").unwrap_err().contains("invalid JSON"));
    assert!(parse_corpus("[]").unwrap_err().contains("not an object"));
    assert!(parse_corpus("").unwrap_err().contains("no cases"));
    let row = CORPUS.lines().find(|line| !line.is_empty()).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(row).unwrap();
    value["schema"] = serde_json::json!("wrong");
    assert!(
        parse_corpus(&value.to_string())
            .unwrap_err()
            .contains("violates the schema")
    );
    assert_eq!(
        parse_corpus(&format!("\n{CORPUS}")).unwrap().len(),
        parse_corpus(CORPUS).unwrap().len()
    );
    let mut item = parse_corpus(CORPUS).unwrap().remove(0);
    item.id = "unknown".into();
    assert!(
        validate_case(&item)
            .unwrap_err()
            .contains("not a recognized")
    );
    assert!(pair_scenario_contract("unknown").is_none());
    item = parse_corpus(CORPUS).unwrap().remove(0);
    item.expected_fallthrough = vec![vec!["unknown".into()]];
    assert!(
        validate_normalized_states(&item)
            .unwrap_err()
            .contains("unknown normalized")
    );
    item.expected_fallthrough.clear();
    item.mode = "strict".into();
    item.expected_loop_head = Some(vec![]);
    assert!(
        validate_normalized_states(&item)
            .unwrap_err()
            .contains("outside an internal")
    );
}

#[test]
fn fixture_reports_each_filesystem_failure() {
    assert_eq!(
        Fixture::from_tempdir("pass", Err(std::io::Error::other("denied")))
            .err()
            .unwrap(),
        "denied"
    );
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("project"), "blocked").unwrap();
    assert!(Fixture::from_tempdir("pass", Ok(directory)).is_err());
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("project/src/case.py")).unwrap();
    assert!(Fixture::from_tempdir("pass", Ok(directory)).is_err());
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("project/pyproject.toml")).unwrap();
    assert!(Fixture::from_tempdir("pass", Ok(directory)).is_err());
}

#[tokio::test(start_paused = true)]
async fn decoder_distinguishes_deadline_exit_and_malformed_payload() {
    let item = parse_corpus(CORPUS).unwrap().remove(0);
    let elapsed =
        tokio::time::timeout(Duration::from_millis(1), std::future::pending::<i32>()).await;
    assert!(
        decode_public_plan(&item, elapsed, b"", b"")
            .unwrap_err()
            .contains("timed out")
    );
    assert!(
        decode_public_plan(&item, Ok(2), b"", b"failed")
            .unwrap_err()
            .contains("exit=2, stderr=failed")
    );
    assert!(
        decode_public_plan(&item, Ok(0), b"", b"warning")
            .unwrap_err()
            .contains("exit=0, stderr=warning")
    );
    assert!(
        decode_public_plan(&item, Ok(0), b"{", b"")
            .unwrap_err()
            .contains("malformed plan")
    );
}

#[tokio::test]
async fn observation_checks_marker_overlap_and_span_overflow() {
    let mut item = parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .find(|item| item.mode == "strict" && item.expected_present)
        .unwrap();
    let manifest = public_plan(&item).await.unwrap();
    let mut changed = manifest.clone();
    changed.candidates.push(changed.candidates[0].clone());
    assert!(
        observe_manifest(&item, &changed)
            .unwrap_err()
            .contains("matching candidates")
    );
    changed = manifest.clone();
    changed.candidates[0].candidate.span.start = u64::MAX;
    changed.candidates[0].candidate.span.length = 1;
    assert!(!observe_manifest(&item, &changed).unwrap().present);
    item.site_marker = "missing".into();
    assert!(
        observe_manifest(&item, &manifest)
            .unwrap_err()
            .contains("marker occurs 0 times")
    );
}
