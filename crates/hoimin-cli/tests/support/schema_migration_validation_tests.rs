use super::*;

fn case_for(scenario: &str) -> OracleCase {
    parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .find(|case| case.scenario == scenario)
        .unwrap()
}

#[test]
fn parser_and_contract_validation_reject_invalid_rows() {
    assert!(parse_corpus("{").unwrap_err().contains("invalid JSON"));
    assert!(parse_corpus("").unwrap_err().contains("no cases"));
    assert_eq!(
        parse_corpus(&format!("\n{CORPUS}")).unwrap().len(),
        parse_corpus(CORPUS).unwrap().len()
    );
    let mut case = case_for("concurrent_fresh");
    case.schema = 2;
    assert!(validate_case(&case).unwrap_err().contains("invalid schema"));
    case = case_for("concurrent_fresh");
    case.scenario = "unknown".into();
    assert!(
        validate_case(&case)
            .unwrap_err()
            .contains("mismatched correspondence")
    );
    assert!(!known_event("invalid"));
    assert!(!known_event("observe:unknown"));
    assert!(!known_event("unknown:left"));
    assert!(
        expected_version_number("unknown")
            .unwrap_err()
            .contains("unknown expected version")
    );
    assert_eq!(expected_version_number("fresh").unwrap(), 0);
}

#[test]
fn strict_execution_reports_expected_result_version_and_marker_mismatches() {
    let mut case = case_for("concurrent_fresh");
    case.expected_open_results = vec!["wrong".into(), "wrong".into()];
    assert!(
        run_strict_case(&case)
            .unwrap_err()
            .contains("open results mismatch")
    );
    case = case_for("concurrent_fresh");
    case.expected_version = "fresh".into();
    assert!(
        run_strict_case(&case)
            .unwrap_err()
            .contains("version mismatch")
    );
    case = case_for("concurrent_fresh");
    case.expected_legacy_row = true;
    assert!(
        run_strict_case(&case)
            .unwrap_err()
            .contains("marker mismatch")
    );
    case = case_for("stale_reread");
    assert!(
        run_strict_case(&case)
            .unwrap_err()
            .contains("cannot prepare")
    );
    assert_eq!(
        run_strict_case_from(&case, Err(std::io::Error::other("denied"))).unwrap_err(),
        "denied"
    );
}

#[test]
fn fixture_and_marker_queries_report_storage_errors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("database");
    std::fs::create_dir(&path).unwrap();
    for scenario in ["rollback", "unsupported"] {
        assert!(prepare_fixture(&case_for(scenario), &path).is_err());
    }
    let path = directory.path().join("existing.sqlite3");
    let case = case_for("rollback");
    prepare_fixture(&case, &path).unwrap();
    assert!(prepare_fixture(&case, &path).is_err());
    let path = directory.path().join("unsupported.sqlite3");
    let case = case_for("unsupported");
    prepare_fixture(&case, &path).unwrap();
    assert!(prepare_fixture(&case, &path).is_err());
    let connection = Connection::open_in_memory().unwrap();
    assert!(observe_marker(&case, &connection).is_err());
    let mut case = case;
    case.scenario = "unknown".into();
    assert!(
        observe_marker(&case, &connection)
            .unwrap_err()
            .contains("cannot observe marker")
    );
    assert!(open_observation_connection(directory.path()).is_err());
    let sql_error = connection.execute("not SQL", []).unwrap_err();
    let database_error = Err(SessionError::Schema(SchemaError::Database(sql_error)));
    assert_eq!(classify_open(database_error, None), "database_error");
    let path = directory.path().join("unsupported.sqlite3");
    assert!(classify_open(SessionHandler::open(&path), None).starts_with("unexpected_error:"));
}

#[test]
fn concurrent_collection_reports_channel_and_thread_failures() {
    let (sender, receiver) = mpsc::channel();
    drop(sender);
    assert!(
        collect_open_results(&receiver, vec![], Duration::from_millis(1))
            .unwrap_err()
            .contains("concurrent open timed out")
    );
    let (sender, receiver) = mpsc::channel();
    sender.send("ok".into()).unwrap();
    sender.send("ok".into()).unwrap();
    drop(sender);
    let thread = thread::spawn(|| panic!("fixture thread"));
    assert_eq!(
        collect_open_results(&receiver, vec![thread], Duration::from_secs(1)).unwrap_err(),
        "concurrent open thread panicked"
    );
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
    assert!(
        run_strict_case(&case_for("rollback"))
            .unwrap_err()
            .contains("strict case has unsupported scenario rollback")
    );
}
