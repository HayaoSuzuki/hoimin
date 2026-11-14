use super::*;

#[test]
fn validator_rejects_schedule_length() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mut changed = case.clone();
    changed.schedule.clear();

    let actual_0 = validate_case(&changed);

    assert!(actual_0.unwrap_err().contains("invalid schedule length"));
}

#[rstest::rstest]
#[case::event(|case: &mut OracleCase| case.expected[0].event = "different".into(), "mismatched observation event")]
#[case::verdict(|case: &mut OracleCase| case.expected[0].verdict = "unknown".into(), "unknown verdict")]
fn validator_rejects_observation_fields(
    #[case] damage: fn(&mut OracleCase),
    #[case] expected: &str,
) {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    damage(&mut case);

    let result = validate_case(&case);

    assert!(result.unwrap_err().contains(expected));
}

#[test]
fn validator_rejects_unknown_error_code() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mut changed = case.clone();
    changed.expected[0].error_code = Some("unknown".to_owned());

    let actual_0 = validate_case(&changed);

    assert!(actual_0.unwrap_err().contains("unknown error code"));
}

#[test]
fn validator_rejects_unknown_state_role() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mut changed = case.clone();
    changed.expected[0].selected_run = Some("r9".to_owned());

    let actual_0 = validate_case(&changed);

    assert!(actual_0.unwrap_err().contains("unknown state role"));
}

#[test]
fn validator_rejects_unknown_result_role() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mut changed = case;
    changed.expected[0].results.push(ResultObservation {
        run: "r0".into(),
        mutant: "m0".into(),
        status: "unknown".into(),
        payload: "p0".into(),
    });

    let actual_0 = validate_case(&changed);

    assert!(actual_0.unwrap_err().contains("unknown result role"));
}

#[test]
fn parser_rejects_invalid_json() {
    let actual_0 = parse_corpus("{");

    assert!(actual_0.unwrap_err().contains("invalid JSON"));
}

#[test]
fn parser_rejects_empty_corpus() {
    let actual_0 = parse_corpus("");

    assert!(actual_0.is_err());
}

#[test]
fn run_identity_rejects_unknown_role() {
    let actual_0 = run_id("r9");

    assert!(actual_0.unwrap_err().contains("unknown"));
}

#[test]
fn run_role_rejects_unknown_identity() {
    let actual_0 = run_role("unexpected");

    assert!(actual_0.unwrap_err().contains("unknown"));
}

#[test]
fn mutant_identity_rejects_unknown_role() {
    let actual_0 = mutant_id("m9");

    assert!(actual_0.unwrap_err().contains("unknown"));
}

#[test]
fn mutant_role_rejects_unknown_identity() {
    let actual_0 = mutant_role("unexpected");

    assert!(actual_0.unwrap_err().contains("unknown"));
}

#[test]
fn fingerprint_rejects_unknown_role() {
    let actual_0 = fingerprint("f9");

    assert!(actual_0.is_err());
}

#[test]
fn fingerprint_role_rejects_unknown_digest() {
    let actual_0 = fingerprint_role(&[9; 32]);

    assert!(actual_0.is_err());
}

#[test]
fn status_rejects_unknown_name() {
    let actual_0 = status("unknown");

    assert!(actual_0.is_err());
}

#[test]
fn persist_rejects_unknown_payload() {
    let actual_0 = persist_request(0, "r0", "m0", "killed", "p9", "valid");

    assert!(actual_0.unwrap_err().contains("unknown payload"));
}

#[rstest::rstest]
#[case::case_0("killed")]
#[case::case_1("survived")]
#[case::case_2("not_run")]
#[case::case_3("timeout")]
#[case::case_4("out_of_memory")]
#[case::case_5("process_limit")]
#[case::case_6("error")]
fn status_supports_closed_names(#[case] name: &str) {
    let request = persist_request(0, "r0", "m0", name, "p0", "valid").unwrap();

    let actual_0 = request.result.outcome.status().as_str();

    assert_eq!(actual_0, name);
}

#[test]
fn driver_reports_tempdir_failure() {
    let actual_0 = Driver::from_tempdir(Err(std::io::Error::other("denied")));

    assert!(actual_0.err().unwrap().contains("create temp dir: denied"));
}

#[test]
fn driver_rejects_unopened_handler() {
    let mut driver = Driver::new().unwrap();

    let actual_0 = driver.handler("h0");

    assert!(actual_0.err().unwrap().contains("unopened handler h0"));
}

#[test]
fn driver_rejects_unknown_event() {
    let mut driver = Driver::new().unwrap();

    let actual_0 = driver.apply("unknown");

    assert!(actual_0.unwrap_err().contains("unknown"));
}

#[test]
fn classification_preserves_infrastructure_details() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let error = classify_execution(&case, Ok(Err("database unavailable".into())));

    let actual_1 = error.detail.as_deref();

    assert_eq!(error.class, CaseClass::InfrastructureError);
    assert_eq!(actual_1, Some("database unavailable"));
}

#[test]
fn classification_preserves_observation_count_mismatch() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mismatch = classify_execution(&case, Ok(Ok(vec![])));

    let actual_1 = mismatch_detail(&mismatch);

    assert_eq!(mismatch.class, CaseClass::Mismatch);
    assert!(actual_1.contains("observation count differs"));
}

#[test]
fn classification_preserves_step_mismatch() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mut actual = case.expected.clone();
    actual[0].verdict = "different".into();
    let different = classify_execution(&case, Ok(Ok(actual)));

    let actual_0 = mismatch_detail(&different);

    assert!(actual_0.contains("step 0"));
}

#[test]
fn correspondence_rejects_failed_cases() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let error = classify_execution(&case, Ok(Err("database unavailable".into())));
    let mismatch = classify_execution(&case, Ok(Ok(vec![])));
    let mut actual = case.expected.clone();
    actual[0].verdict = "different".into();
    let different = classify_execution(&case, Ok(Ok(actual)));

    let actual_0 =
        std::panic::catch_unwind(|| assert_correspondence(&[error, mismatch, different]));

    assert!(actual_0.is_err());
}

#[test]
fn database_reports_missing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observation.sqlite3");

    let actual_0 = observe_database(&path);

    assert!(
        actual_0
            .unwrap_err()
            .contains("open observation connection")
    );
}

#[test]
fn database_reports_missing_runs() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observation.sqlite3");
    let _connection = Connection::open(&path).unwrap();

    let actual_0 = observe_database(&path);

    assert!(actual_0.unwrap_err().contains("prepare run observation"));
}

#[test]
fn database_reports_missing_results() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observation.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE runs(id,run_id,fingerprint,finished,complete);")
        .unwrap();

    let actual_0 = observe_database(&path);

    assert!(actual_0.unwrap_err().contains("prepare result observation"));
}

#[test]
fn database_reports_invalid_fingerprint_storage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observation.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE runs(id,run_id,fingerprint,finished,complete);")
        .unwrap();
    connection.execute_batch("CREATE TABLE results(run_id,mutant_id,outcome_json); CREATE TABLE candidates(run_id,mutant_id,replacement);").unwrap();
    connection
        .execute(
            "INSERT INTO runs VALUES (1,'oracle-r0','not a blob',0,0)",
            [],
        )
        .unwrap();

    let actual_0 = observe_database(&path);

    assert!(actual_0.unwrap_err().contains("read run observation"));
}

#[test]
fn database_reports_invalid_run_ordinal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observation.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE runs(id,run_id,fingerprint,finished,complete);")
        .unwrap();
    connection.execute_batch("CREATE TABLE results(run_id,mutant_id,outcome_json); CREATE TABLE candidates(run_id,mutant_id,replacement);").unwrap();
    connection
        .execute(
            "INSERT INTO runs VALUES (0,'oracle-r0',?1,0,0)",
            [&[1u8; 32][..]],
        )
        .unwrap();

    let actual_0 = observe_database(&path);

    assert!(actual_0.unwrap_err().contains("invalid run ordinal"));
}

#[test]
fn database_reports_invalid_result_payload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observation.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE runs(id,run_id,fingerprint,finished,complete);")
        .unwrap();
    connection.execute_batch("CREATE TABLE results(run_id,mutant_id,outcome_json); CREATE TABLE candidates(run_id,mutant_id,replacement);").unwrap();
    connection.execute_batch("DELETE FROM runs; INSERT INTO results VALUES ('oracle-r0','oracle-m0','not json'); INSERT INTO candidates VALUES ('oracle-r0','oracle-m0',NULL)").unwrap();

    let actual_0 = observe_database(&path);

    assert!(actual_0.unwrap_err().contains("read result observation"));
}

#[rstest::rstest]
#[case::unrecognized("wrong", "unknown replacement payload")]
#[case::unknown_role("replacement-p9", "unknown replacement payload")]
#[case::invalid_outcome("replacement-p0", "invalid outcome")]
fn database_reports_invalid_replacement(#[case] replacement: &str, #[case] expected: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observation.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE runs(id,run_id,fingerprint,finished,complete);")
        .unwrap();
    connection.execute_batch("CREATE TABLE results(run_id,mutant_id,outcome_json); CREATE TABLE candidates(run_id,mutant_id,replacement);").unwrap();
    connection.execute_batch("DELETE FROM runs; INSERT INTO results VALUES ('oracle-r0','oracle-m0','not json'); INSERT INTO candidates VALUES ('oracle-r0','oracle-m0',NULL)").unwrap();
    connection
        .execute("UPDATE candidates SET replacement=?1", [replacement])
        .unwrap();

    let actual_0 = observe_database(&path);

    assert!(actual_0.unwrap_err().contains(expected));
}

#[test]
fn reopened_handler_releases_ownership() {
    let mut driver = Driver::new().unwrap();
    driver.open_handler("h0").unwrap();
    driver.begin("h0", "r0", "f0", 0).unwrap();
    driver.open_handler("h0").unwrap();

    let actual_0 = driver.owners.is_empty();

    assert!(actual_0);
}

#[test]
fn driver_rejects_second_begin() {
    let mut driver = Driver::new().unwrap();
    driver.open_handler("h0").unwrap();
    driver.begin("h0", "r0", "f0", 0).unwrap();
    driver.open_handler("h0").unwrap();
    driver.begin("h0", "r0", "f0", 1).unwrap();

    let rejected = driver.begin("h0", "r0", "f0", 2).unwrap();

    assert_eq!(rejected.0, "rejected");
}

#[test]
fn driver_rejects_finish_for_other_run() {
    let mut driver = Driver::new().unwrap();
    driver.open_handler("h0").unwrap();
    driver.begin("h0", "r0", "f0", 0).unwrap();
    driver.open_handler("h0").unwrap();
    driver.begin("h0", "r0", "f0", 1).unwrap();

    let rejected = driver.finish("h0", "r1", "true", 3).unwrap();

    assert_eq!(rejected.0, "rejected");
}

#[test]
fn driver_reports_storage_failure() {
    let mut broken = Driver::new().unwrap();
    std::fs::create_dir(&broken.path).unwrap();

    let actual_0 = broken.open_handler("h0");

    assert!(actual_0.unwrap_err().contains("open handler h0"));
}

#[test]
fn parser_ignores_empty_lines() {
    let input = format!("\n{}", corpus_text());

    let actual_0 = parse_corpus(&input);

    assert_eq!(
        actual_0.unwrap().len(),
        parse_corpus(corpus_text()).unwrap().len()
    );
}

#[test]
fn selector_selects_identity() {
    let cases = parse_corpus(corpus_text()).unwrap();

    let actual_0 = select_cases(&cases, Some(&cases[0].id));

    assert_eq!(actual_0.len(), 1);
}

#[test]
fn selector_rejects_unknown_identity() {
    let cases = parse_corpus(corpus_text()).unwrap();

    let actual_0 = std::panic::catch_unwind(|| select_cases(&cases, Some("unknown")));

    assert!(actual_0.is_err());
}

#[test]
fn finish_accepts_without_owner_tracking() {
    let mut driver = Driver::new().unwrap();
    driver.open_handler("h0").unwrap();
    driver.begin("h0", "r0", "f0", 0).unwrap();
    driver.owners.clear();

    let actual_0 = driver.finish("h0", "r0", "true", 1);

    assert_eq!(actual_0.unwrap().0, "accepted");
}
