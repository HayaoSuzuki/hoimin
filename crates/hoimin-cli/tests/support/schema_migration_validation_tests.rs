use super::*;

fn case_for(scenario: &str) -> OracleCase {
    parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .find(|case| case.scenario == scenario)
        .unwrap()
}

#[test]
fn parser_rejects_json() {
    let actual_0 = parse_corpus("{");

    assert!(actual_0.unwrap_err().contains("invalid JSON"));
}

#[test]
fn parser_rejects_empty_corpus() {
    let actual_0 = parse_corpus("");

    assert!(actual_0.unwrap_err().contains("no cases"));
}

#[test]
fn parser_ignores_empty_lines() {
    let actual_0 = parse_corpus(&format!("\n{CORPUS}"));

    assert_eq!(actual_0.unwrap().len(), parse_corpus(CORPUS).unwrap().len());
}

#[test]
fn validator_rejects_schema() {
    let mut case = case_for("concurrent_fresh");
    case.schema = 2;

    let actual_0 = validate_case(&case);

    assert!(actual_0.unwrap_err().contains("invalid schema"));
}

#[test]
fn validator_rejects_scenario() {
    let mut case = case_for("concurrent_fresh");
    case.scenario = "unknown".into();

    let actual_0 = validate_case(&case);

    assert!(actual_0.unwrap_err().contains("mismatched correspondence"));
}

#[test]
fn event_rejects_invalid_syntax() {
    let actual_0 = known_event("invalid");

    assert!(!actual_0);
}

#[test]
fn event_rejects_unknown_observation() {
    let actual_0 = known_event("observe:unknown");

    assert!(!actual_0);
}

#[test]
fn event_rejects_unknown_action() {
    let actual_0 = known_event("unknown:left");

    assert!(!actual_0);
}

#[test]
fn version_rejects_unknown_label() {
    let actual_0 = expected_version_number("unknown");

    assert!(actual_0.unwrap_err().contains("unknown expected version"));
}

#[test]
fn version_maps_fresh_to_zero() {
    let actual_0 = expected_version_number("fresh");

    assert_eq!(actual_0.unwrap(), 0);
}

#[test]
fn strict_execution_rejects_open_result_mismatch() {
    let mut case = case_for("concurrent_fresh");
    case.expected_open_results = vec!["wrong".into(), "wrong".into()];

    let actual_0 = run_strict_case(&case);

    assert!(actual_0.unwrap_err().contains("open results mismatch"));
}

#[test]
fn strict_execution_rejects_version_mismatch() {
    let mut case = case_for("concurrent_fresh");
    case.expected_version = "fresh".into();

    let actual_0 = run_strict_case(&case);

    assert!(actual_0.unwrap_err().contains("version mismatch"));
}

#[test]
fn strict_execution_rejects_marker_mismatch() {
    let mut case = case_for("concurrent_fresh");
    case.expected_legacy_row = true;

    let actual_0 = run_strict_case(&case);

    assert!(actual_0.unwrap_err().contains("marker mismatch"));
}

#[test]
fn strict_execution_rejects_unprepared_scenario() {
    let case = case_for("stale_reread");

    let actual_0 = run_strict_case(&case);

    assert!(actual_0.unwrap_err().contains("cannot prepare"));
}

#[test]
fn strict_execution_reports_tempdir_error() {
    let case = case_for("stale_reread");

    let actual_0 = run_strict_case_from(&case, Err(std::io::Error::other("denied")));

    assert_eq!(actual_0.unwrap_err(), "denied");
}

#[rstest::rstest]
#[case::case_0("rollback")]
#[case::case_1("unsupported")]
fn fixture_rejects_directory_database(#[case] scenario: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("database");
    std::fs::create_dir(&path).unwrap();

    let actual_0 = prepare_fixture(&case_for(scenario), &path);

    assert!(actual_0.is_err());
}

#[test]
fn fixture_rejects_existing_rollback_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.sqlite3");
    let case = case_for("rollback");
    prepare_fixture(&case, &path).unwrap();

    let actual_0 = prepare_fixture(&case, &path);

    assert!(actual_0.is_err());
}

#[test]
fn fixture_rejects_existing_unsupported_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unsupported.sqlite3");
    let case = case_for("unsupported");
    prepare_fixture(&case, &path).unwrap();

    let actual_0 = prepare_fixture(&case, &path);

    assert!(actual_0.is_err());
}

#[test]
fn marker_reports_missing_schema() {
    let case = case_for("unsupported");
    let connection = Connection::open_in_memory().unwrap();

    let actual_0 = observe_marker(&case, &connection);

    assert!(actual_0.is_err());
}

#[test]
fn marker_rejects_unknown_scenario() {
    let case = case_for("unsupported");
    let connection = Connection::open_in_memory().unwrap();
    let mut case = case;
    case.scenario = "unknown".into();

    let actual_0 = observe_marker(&case, &connection);

    assert!(actual_0.unwrap_err().contains("cannot observe marker"));
}

#[test]
fn observation_rejects_directory_database() {
    let directory = tempfile::tempdir().unwrap();

    let actual_0 = open_observation_connection(directory.path());

    assert!(actual_0.is_err());
}

#[test]
fn open_classifies_database_error() {
    let connection = Connection::open_in_memory().unwrap();
    let sql_error = connection.execute("not SQL", []).unwrap_err();
    let database_error = Err(SessionError::Schema(SchemaError::Database(sql_error)));

    let actual_0 = classify_open(database_error, None);

    assert_eq!(actual_0, "database_error");
}

#[test]
fn open_classifies_unsupported_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unsupported.sqlite3");
    let case = case_for("unsupported");
    prepare_fixture(&case, &path).unwrap();
    let path = directory.path().join("unsupported.sqlite3");

    let actual_0 = classify_open(SessionHandler::open(&path), None);

    assert!(actual_0.starts_with("unexpected_error:"));
}

#[test]
fn concurrent_collection_reports_closed_channel() {
    let (sender, receiver) = mpsc::channel();
    drop(sender);

    let actual_0 = collect_open_results(&receiver, vec![], Duration::from_millis(1));

    assert!(actual_0.unwrap_err().contains("concurrent open timed out"));
}

#[test]
fn concurrent_collection_reports_panicked_thread() {
    let (sender, receiver) = mpsc::channel();
    sender.send("ok".into()).unwrap();
    sender.send("ok".into()).unwrap();
    drop(sender);
    let thread = thread::spawn(|| panic!("fixture thread"));

    let actual_0 = collect_open_results(&receiver, vec![thread], Duration::from_secs(1));

    assert_eq!(actual_0.unwrap_err(), "concurrent open thread panicked");
}

#[test]
fn version_query_reports_denied_pragma() {
    unsafe extern "C" fn deny(
        _: *mut std::ffi::c_void,
        _: std::ffi::c_int,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
    ) -> std::ffi::c_int {
        rusqlite::ffi::SQLITE_DENY
    }
    let connection = Connection::open_in_memory().unwrap();

    // This exclusive test connection deliberately rejects SQL authorization; no global state changes.
    unsafe {
        rusqlite::ffi::sqlite3_set_authorizer(
            connection.handle(),
            Some(deny),
            std::ptr::null_mut(),
        );
    }
    assert!(user_version(&connection).is_err());
}

#[test]
fn rollback_is_not_a_public_strict_execution_scenario() {
    let actual_0 = run_strict_case(&case_for("rollback"));

    assert!(
        actual_0
            .unwrap_err()
            .contains("strict case has unsupported scenario rollback")
    );
}
