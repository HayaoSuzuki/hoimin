use super::*;

#[test]
fn parser_rejects_invalid_schema_roles_and_observations() {
    let rows = corpus_text()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    for (field, value, expected) in [
        ("schema", serde_json::json!(2), "unsupported schema"),
        ("mode", serde_json::json!("unknown"), "unknown mode"),
        ("schedule", serde_json::json!([]), "invalid schedule length"),
    ] {
        let mut changed = rows.clone();
        changed[0][field] = value;
        let input = changed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_corpus(&input).unwrap_err().contains(expected));
    }
    for (field, value, expected) in [
        (
            "event",
            serde_json::json!("wrong"),
            "observation event does not match",
        ),
        ("verdict", serde_json::json!("wrong"), "unknown verdict"),
        (
            "active",
            serde_json::json!([{"worker":"w9","generation":"g0"}]),
            "unknown worker",
        ),
        (
            "active",
            serde_json::json!([{"worker":"w0","generation":"g9"}]),
            "unknown generation",
        ),
    ] {
        let mut changed = rows.clone();
        changed[0]["expected"][0][field] = value;
        let input = changed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_corpus(&input).unwrap_err().contains(expected));
    }
    let mut changed = rows.clone();
    changed[0]["schedule"][0] = serde_json::json!("unknown");
    assert!(
        parse_corpus(
            &changed
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
        .unwrap_err()
        .contains("unknown event")
    );
    assert!(parse_corpus("").unwrap_err().contains("expected 6 cases"));
}

#[test]
fn fixture_rejects_filesystem_obstructions_and_invalid_ordering() {
    assert_eq!(
        StrictFixture::from_tempdir(Err(std::io::Error::other("denied")))
            .err()
            .unwrap(),
        "denied"
    );
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("pkg"), "blocked").unwrap();
    assert!(StrictFixture::from_tempdir(Ok(directory)).is_err());
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("pkg/a.py")).unwrap();
    assert!(writeproject_source(&directory.path().join("pkg")).is_err());
    let mut fixture = StrictFixture::new().unwrap();
    assert!(
        fixture
            .create("g0")
            .unwrap_err()
            .contains("before preflight grant")
    );
    assert!(
        fixture
            .cleanup()
            .unwrap_err()
            .contains("before preflight grant")
    );
    assert!(
        fixture
            .candidate()
            .unwrap_err()
            .contains("without active worker")
    );
    fixture.preflight().unwrap();
    fixture.create("g0").unwrap();
    fixture.generations.clear();
    assert!(
        fixture
            .active()
            .unwrap_err()
            .contains("unmapped worker root")
    );
    let mut fixture = StrictFixture::new().unwrap();
    fs::remove_file(fixture.project.path().join("pkg/a.py")).unwrap();
    fixture.preflight().unwrap();
    fixture.create("g0").unwrap();
    assert!(
        fixture
            .candidate()
            .unwrap_err()
            .contains("absent from manifest")
    );
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.schedule = vec!["unknown".into()];
    assert!(
        replay_strict(&case)
            .unwrap_err()
            .contains("unknown strict event")
    );
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
    assert!(fixture.create("g0").is_err());
    assert_eq!(
        fixture.observe("create:w0:g0", None).unwrap_err(),
        "strict fixture cannot inspect a pending generation"
    );
    fixture.handler.close().unwrap();
}

#[cfg(unix)]
#[test]
fn non_unicode_fixture_path_is_rejected() {
    use std::os::unix::ffi::OsStringExt;
    let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![255]));
    assert_eq!(
        utf8project_root(&path).unwrap_err(),
        "temporary project path is not UTF-8"
    );
}

#[test]
fn workspace_error_callbacks_preserve_actual_failure_codes() {
    let mut fixture = StrictFixture::new().unwrap();
    assert!(
        created_worker_root(&fixture.handler)
            .unwrap_err()
            .contains("no active worker")
    );
    assert!(fixture.reset().is_err());
    let completed = fixture
        .handler
        .handle_preflight(Preflight { id: EffectId(1) })
        .unwrap();
    let mut exhausted = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 0,
        processes: 1,
    });
    assert!(reserve_grant(&mut exhausted, &completed).is_err());
    let mut empty = completed.clone();
    empty.requested_workers = 0;
    empty.aggregate_logical_bytes = 0;
    fixture.grant = Some(reserve_grant(&mut exhausted, &empty).unwrap());
    assert!(fixture.create("g0").is_err());
    fixture.preflight().unwrap();
    fixture.create("g0").unwrap();
    fixture
        .handler
        .worker_mut(0)
        .unwrap()
        .write(Utf8Path::new("pkg/a.py"), b"changed!\n")
        .unwrap();
    assert!(fixture.apply().is_err());
    let mut alternate = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    alternate.reserve(hoimin_core::BudgetKind::Copy, 0).unwrap();
    fixture.grant = Some(reserve_grant(&mut alternate, &completed).unwrap());
    assert!(fixture.cleanup().is_err());
    let mut fixture = StrictFixture::new().unwrap();
    fs::remove_dir_all(fixture.project.path()).unwrap();
    assert!(fixture.preflight().is_err());
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    assert!(selected_workspace_case(&case, Some(&case.id)));
    assert!(!selected_workspace_case(&case, Some("unknown")));
}
