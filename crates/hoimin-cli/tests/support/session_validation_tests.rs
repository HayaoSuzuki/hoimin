use super::*;

#[test]
fn validation_rejects_observation_contract_violations() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mut changed = case.clone();
    changed.schedule.clear();
    assert!(
        validate_case(&changed)
            .unwrap_err()
            .contains("invalid schedule length")
    );
    for (field, value, expected) in [
        ("event", "different", "mismatched observation event"),
        ("verdict", "unknown", "unknown verdict"),
    ] {
        let mut changed = case.clone();
        if field == "event" {
            changed.expected[0].event = value.to_owned();
        } else {
            changed.expected[0].verdict = value.to_owned();
        }
        assert!(validate_case(&changed).unwrap_err().contains(expected));
    }
    let mut changed = case.clone();
    changed.expected[0].error_code = Some("unknown".to_owned());
    assert!(
        validate_case(&changed)
            .unwrap_err()
            .contains("unknown error code")
    );
    changed = case.clone();
    changed.expected[0].selected_run = Some("r9".to_owned());
    assert!(
        validate_case(&changed)
            .unwrap_err()
            .contains("unknown state role")
    );
    changed = case;
    changed.expected[0].results.push(ResultObservation {
        run: "r0".into(),
        mutant: "m0".into(),
        status: "unknown".into(),
        payload: "p0".into(),
    });
    assert!(
        validate_case(&changed)
            .unwrap_err()
            .contains("unknown result role")
    );
    assert!(parse_corpus("{").unwrap_err().contains("invalid JSON"));
    assert!(parse_corpus("").is_err());
}

#[test]
fn identity_and_payload_conversions_reject_unknown_roles() {
    assert!(run_id("r9").unwrap_err().contains("unknown"));
    assert!(run_role("unexpected").unwrap_err().contains("unknown"));
    assert!(mutant_id("m9").unwrap_err().contains("unknown"));
    assert!(mutant_role("unexpected").unwrap_err().contains("unknown"));
    assert!(fingerprint("f9").is_err());
    assert!(fingerprint_role(&[9; 32]).is_err());
    assert!(status("unknown").is_err());
    assert!(
        persist_request(0, "r0", "m0", "killed", "p9", "valid")
            .unwrap_err()
            .contains("unknown payload")
    );
    for name in [
        "killed",
        "survived",
        "not_run",
        "timeout",
        "out_of_memory",
        "process_limit",
        "error",
    ] {
        let request = persist_request(0, "r0", "m0", name, "p0", "valid").unwrap();
        assert_eq!(request.result.outcome.status().as_str(), name);
    }
    assert!(
        Driver::from_tempdir(Err(std::io::Error::other("denied")))
            .err()
            .unwrap()
            .contains("create temp dir: denied")
    );
    let mut driver = Driver::new().unwrap();
    assert!(
        driver
            .handler("h0")
            .err()
            .unwrap()
            .contains("unopened handler h0")
    );
    assert!(driver.apply("unknown").unwrap_err().contains("unknown"));
}

#[test]
fn classification_and_failure_report_preserve_error_details() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let error = classify_execution(&case, Ok(Err("database unavailable".into())));
    assert_eq!(error.class, CaseClass::InfrastructureError);
    assert_eq!(error.detail.as_deref(), Some("database unavailable"));
    let mismatch = classify_execution(&case, Ok(Ok(vec![])));
    assert_eq!(mismatch.class, CaseClass::Mismatch);
    assert!(mismatch_detail(&mismatch).contains("observation count differs"));
    let mut actual = case.expected.clone();
    actual[0].verdict = "different".into();
    let different = classify_execution(&case, Ok(Ok(actual)));
    assert!(mismatch_detail(&different).contains("step 0"));
    assert!(
        std::panic::catch_unwind(|| assert_correspondence(&[error, mismatch, different])).is_err()
    );
}

#[test]
fn database_observation_reports_corrupt_and_incomplete_storage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observation.sqlite3");
    assert!(
        observe_database(&path)
            .unwrap_err()
            .contains("open observation connection")
    );
    let connection = Connection::open(&path).unwrap();
    assert!(
        observe_database(&path)
            .unwrap_err()
            .contains("prepare run observation")
    );
    connection
        .execute_batch("CREATE TABLE runs(id,run_id,fingerprint,finished,complete);")
        .unwrap();
    assert!(
        observe_database(&path)
            .unwrap_err()
            .contains("prepare result observation")
    );
    connection.execute_batch("CREATE TABLE results(run_id,mutant_id,outcome_json); CREATE TABLE candidates(run_id,mutant_id,replacement);").unwrap();
    connection
        .execute(
            "INSERT INTO runs VALUES (1,'oracle-r0','not a blob',0,0)",
            [],
        )
        .unwrap();
    assert!(
        observe_database(&path)
            .unwrap_err()
            .contains("read run observation")
    );
    connection.execute_batch("DELETE FROM runs").unwrap();
    connection
        .execute(
            "INSERT INTO runs VALUES (0,'oracle-r0',?1,0,0)",
            [&[1u8; 32][..]],
        )
        .unwrap();
    assert!(
        observe_database(&path)
            .unwrap_err()
            .contains("invalid run ordinal")
    );
    connection.execute_batch("DELETE FROM runs; INSERT INTO results VALUES ('oracle-r0','oracle-m0','not json'); INSERT INTO candidates VALUES ('oracle-r0','oracle-m0',NULL)").unwrap();
    assert!(
        observe_database(&path)
            .unwrap_err()
            .contains("read result observation")
    );
    for replacement in ["wrong", "replacement-p9", "replacement-p0"] {
        connection
            .execute("UPDATE candidates SET replacement=?1", [replacement])
            .unwrap();
        let expected = if replacement == "replacement-p0" {
            "invalid outcome"
        } else {
            "unknown replacement payload"
        };
        assert!(observe_database(&path).unwrap_err().contains(expected));
    }
}

#[test]
fn driver_reports_storage_failure_and_rejected_begin_finish() {
    let mut driver = Driver::new().unwrap();
    driver.open_handler("h0").unwrap();
    driver.begin("h0", "r0", "f0", 0).unwrap();
    driver.open_handler("h0").unwrap();
    assert!(driver.owners.is_empty());
    driver.begin("h0", "r0", "f0", 1).unwrap();
    let rejected = driver.begin("h0", "r0", "f0", 2).unwrap();
    assert_eq!(rejected.0, "rejected");
    let rejected = driver.finish("h0", "r1", "true", 3).unwrap();
    assert_eq!(rejected.0, "rejected");
    let mut broken = Driver::new().unwrap();
    std::fs::create_dir(&broken.path).unwrap();
    assert!(
        broken
            .open_handler("h0")
            .unwrap_err()
            .contains("open handler h0")
    );
    let input = format!("\n{}", corpus_text());
    assert_eq!(
        parse_corpus(&input).unwrap().len(),
        parse_corpus(corpus_text()).unwrap().len()
    );
}

#[test]
fn selector_rejects_unknown_case_and_finish_tolerates_absent_tracking() {
    let cases = parse_corpus(corpus_text()).unwrap();
    assert_eq!(select_cases(&cases, Some(&cases[0].id)).len(), 1);
    assert!(std::panic::catch_unwind(|| select_cases(&cases, Some("unknown"))).is_err());
    let mut driver = Driver::new().unwrap();
    driver.open_handler("h0").unwrap();
    driver.begin("h0", "r0", "f0", 0).unwrap();
    driver.owners.clear();
    assert_eq!(driver.finish("h0", "r0", "true", 1).unwrap().0, "accepted");
}
