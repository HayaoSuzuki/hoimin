use super::*;

#[rstest::rstest]
#[case::case_0("schema", serde_json::json!(2), "unsupported schema")]
#[case::case_1("mode", serde_json::json!("unknown"), "unknown mode")]
#[case::case_2("schedule", serde_json::json!([]), "invalid schedule length")]
fn parser_rejects_invalid_case_field(
    #[case] field: &str,
    #[case] value: serde_json::Value,
    #[case] expected: &str,
) {
    let rows = corpus_text()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut changed = rows.clone();
    changed[0][field] = value;
    let input = changed
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    let actual_0 = parse_corpus(&input);

    assert!(actual_0.unwrap_err().contains(expected));
}

#[rstest::rstest]
#[case::case_0(
            "event",
            serde_json::json!("wrong"),
            "observation event does not match",
        )]
#[case::case_1("verdict", serde_json::json!("wrong"), "unknown verdict")]
#[case::case_2(
            "active",
            serde_json::json!([{"worker":"w9","generation":"g0"}]),
            "unknown worker",
        )]
#[case::case_3(
            "active",
            serde_json::json!([{"worker":"w0","generation":"g9"}]),
            "unknown generation",
        )]
fn parser_rejects_invalid_observation_field(
    #[case] field: &str,
    #[case] value: serde_json::Value,
    #[case] expected: &str,
) {
    let rows = corpus_text()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();

    let mut changed = rows.clone();
    changed[0]["expected"][0][field] = value;
    let input = changed
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    let result = parse_corpus(&input);

    assert!(result.unwrap_err().contains(expected));
}

#[test]
fn parser_rejects_unknown_event() {
    let rows = corpus_text()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut changed = rows.clone();
    changed[0]["schedule"][0] = serde_json::json!("unknown");

    let actual_0 = parse_corpus(
        &changed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    );

    assert!(actual_0.unwrap_err().contains("unknown event"));
}

#[test]
fn parser_rejects_empty_corpus() {
    let actual_0 = parse_corpus("");

    assert!(actual_0.unwrap_err().contains("expected 6 cases"));
}

#[test]
fn fixture_reports_tempdir_failure() {
    let actual_0 = StrictFixture::from_tempdir(Err(std::io::Error::other("denied")));

    assert_eq!(actual_0.err().unwrap(), "denied");
}

#[test]
fn fixture_reports_root_obstruction() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("pkg"), "blocked").unwrap();

    let actual_0 = StrictFixture::from_tempdir(Ok(directory));

    assert!(actual_0.is_err());
}

#[test]
fn fixture_reports_source_obstruction() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("pkg/a.py")).unwrap();

    let actual_0 = writeproject_source(&directory.path().join("pkg"));

    assert!(actual_0.is_err());
}

#[test]
fn create_requires_preflight() {
    let mut fixture = StrictFixture::new().unwrap();

    let actual_0 = fixture.create("g0");

    assert!(actual_0.unwrap_err().contains("before preflight grant"));
}

#[test]
fn cleanup_requires_preflight() {
    let mut fixture = StrictFixture::new().unwrap();

    let actual_0 = fixture.cleanup();

    assert!(actual_0.unwrap_err().contains("before preflight grant"));
}

#[test]
fn candidate_requires_worker() {
    let fixture = StrictFixture::new().unwrap();

    let actual_0 = fixture.candidate();

    assert!(actual_0.unwrap_err().contains("without active worker"));
}

#[test]
fn active_requires_mapped_root() {
    let mut fixture = StrictFixture::new().unwrap();
    fixture.preflight().unwrap();
    fixture.create("g0").unwrap();
    fixture.generations.clear();

    let actual_0 = fixture.active();

    assert!(actual_0.unwrap_err().contains("unmapped worker root"));
}

#[test]
fn candidate_requires_manifest_entry() {
    let mut fixture = StrictFixture::new().unwrap();
    fs::remove_file(fixture.project.path().join("pkg/a.py")).unwrap();
    fixture.preflight().unwrap();
    fixture.create("g0").unwrap();

    let actual_0 = fixture.candidate();

    assert!(actual_0.unwrap_err().contains("absent from manifest"));
}

#[test]
fn replay_rejects_unknown_event() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.schedule = vec!["unknown".into()];

    let actual_0 = replay_strict(&case);

    assert!(actual_0.unwrap_err().contains("unknown strict event"));
}

#[test]
fn pending_generation_is_rejected_by_strict_observer() {
    let mut fixture = StrictFixture::new().unwrap();
    let root = Utf8Path::from_path(fixture.project.path())
        .unwrap()
        .to_owned();
    fixture.handler = WorkspaceHandler::new(root, Vec::new(), 1, CopyOptions::default())
        .with_import_roots(vec!["missing".into()]);
    fixture.preflight().unwrap();
    let created = fixture.create("g0");
    let observed = fixture.observe("create:w0:g0", None);
    fixture.handler.close().unwrap();

    assert!(created.is_err());
    assert_eq!(
        observed.unwrap_err(),
        "strict fixture cannot inspect a pending generation"
    );
}

#[cfg(unix)]
#[test]
fn non_unicode_fixture_path_is_rejected() {
    use std::os::unix::ffi::OsStringExt;
    let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![255]));

    let actual_0 = utf8project_root(&path);

    assert_eq!(actual_0.unwrap_err(), "temporary project path is not UTF-8");
}

#[test]
fn worker_root_requires_active_worker() {
    let fixture = StrictFixture::new().unwrap();

    let actual_0 = created_worker_root(&fixture.handler);

    assert!(actual_0.unwrap_err().contains("no active worker"));
}

#[test]
fn reset_requires_worker() {
    let mut fixture = StrictFixture::new().unwrap();

    let actual_0 = fixture.reset();

    assert!(actual_0.is_err());
}

#[test]
fn reserve_requires_copy_budget() {
    let mut fixture = StrictFixture::new().unwrap();
    let completed = fixture
        .handler
        .handle_preflight(Preflight { id: EffectId(1) })
        .unwrap();
    let mut exhausted = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 0,
        processes: 1,
    });

    let actual_0 = reserve_grant(&mut exhausted, &completed);

    assert!(actual_0.is_err());
}

#[test]
fn create_rejects_zero_worker_grant() {
    let mut fixture = StrictFixture::new().unwrap();
    let completed = fixture
        .handler
        .handle_preflight(Preflight { id: EffectId(1) })
        .unwrap();
    let mut exhausted = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 0,
        processes: 1,
    });
    let mut empty = completed.clone();
    empty.requested_workers = 0;
    empty.aggregate_logical_bytes = 0;
    fixture.grant = Some(reserve_grant(&mut exhausted, &empty).unwrap());

    let actual_0 = fixture.create("g0");

    assert!(actual_0.is_err());
}

#[test]
fn apply_rejects_changed_source() {
    let mut fixture = StrictFixture::new().unwrap();
    fixture.preflight().unwrap();
    fixture.create("g0").unwrap();
    fixture
        .handler
        .worker_mut(0)
        .unwrap()
        .write(Utf8Path::new("pkg/a.py"), b"changed!\n")
        .unwrap();

    let actual_0 = fixture.apply();

    assert!(actual_0.is_err());
}

#[test]
fn cleanup_rejects_wrong_ledger() {
    let mut fixture = StrictFixture::new().unwrap();
    let completed = fixture
        .handler
        .handle_preflight(Preflight { id: EffectId(1) })
        .unwrap();
    fixture.preflight().unwrap();
    fixture.create("g0").unwrap();
    let mut alternate = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    alternate.reserve(hoimin_core::BudgetKind::Copy, 0).unwrap();
    fixture.grant = Some(reserve_grant(&mut alternate, &completed).unwrap());

    let actual_0 = fixture.cleanup();

    assert!(actual_0.is_err());
}

#[test]
fn preflight_reports_missing_project() {
    let mut fixture = StrictFixture::new().unwrap();
    fs::remove_dir_all(fixture.project.path()).unwrap();

    let actual_0 = fixture.preflight();

    assert!(actual_0.is_err());
}

#[test]
fn selector_accepts_matching_identity() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);

    let actual_0 = selected_workspace_case(&case, Some(&case.id));

    assert!(actual_0);
}

#[test]
fn selector_rejects_unknown_identity() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);

    let actual_0 = selected_workspace_case(&case, Some("unknown"));

    assert!(!actual_0);
}
