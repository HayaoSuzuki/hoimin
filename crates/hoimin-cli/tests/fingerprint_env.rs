#[path = "support/environment.rs"]
mod environment;

use environment::{FLAG, Fixture};
use std::ffi::OsStr;

#[tokio::test]
async fn selected_environment_change_reruns_and_matches_fresh_weak_control() {
    let fixture = Fixture::new();
    let first = fixture
        .run("session.sqlite", false, &[FLAG], Some(OsStr::new("1")))
        .await;
    assert_eq!(first.exit_code, 4);
    assert_eq!(first.report["mutants"][0]["status"], "killed");
    assert_eq!(first.metrics["executed"], 1);
    let resumed = fixture
        .run("session.sqlite", true, &[FLAG], Some(OsStr::new("0")))
        .await;
    let fresh = fixture
        .run("fresh.sqlite", false, &[FLAG], Some(OsStr::new("0")))
        .await;
    assert_ne!(
        first.report["run"]["run_id"],
        resumed.report["run"]["run_id"]
    );
    assert_eq!(resumed.report["mutants"][0]["status"], "survived");
    assert_eq!(resumed.metrics["executed"], 1);
    assert_eq!(resumed.report["mutants"][0]["termination"]["Exit"], 0);
    assert_eq!(
        resumed.report["mutants"][0]["candidate"],
        fresh.report["mutants"][0]["candidate"]
    );
    assert_eq!(
        resumed.report["mutants"][0]["status"],
        fresh.report["mutants"][0]["status"]
    );
}

#[tokio::test]
async fn same_values_reuse_and_unselected_changes_keep_existing_behavior() {
    for names in [vec![FLAG], vec![]] {
        let fixture = Fixture::new();
        let first = fixture
            .run("session.sqlite", false, &names, Some(OsStr::new("1")))
            .await;
        let value = if names.is_empty() { "0" } else { "1" };
        let resumed = fixture
            .run("session.sqlite", true, &names, Some(OsStr::new(value)))
            .await;
        assert_eq!(
            first.report["run"]["run_id"],
            resumed.report["run"]["run_id"]
        );
        assert_eq!(resumed.report["mutants"][0]["status"], "killed");
        assert!(resumed.report["mutants"][0]["termination"].is_null());
        assert_eq!(resumed.metrics["executed"], 0);
    }
}

#[tokio::test]
async fn absent_and_empty_values_are_distinct_resume_inputs() {
    for (before, after) in [(None, Some(OsStr::new(""))), (Some(OsStr::new("")), None)] {
        let fixture = Fixture::new();
        let first = fixture.run("session.sqlite", false, &[FLAG], before).await;
        let resumed = fixture.run("session.sqlite", true, &[FLAG], after).await;
        assert_ne!(
            first.report["run"]["run_id"],
            resumed.report["run"]["run_id"]
        );
        assert_ne!(
            first.report["mutants"][0]["status"],
            resumed.report["mutants"][0]["status"]
        );
        assert_eq!(resumed.metrics["executed"], 1);
    }
}

#[tokio::test]
async fn name_set_changes_invalidate_but_order_and_duplicates_do_not() {
    let other = "HOIMIN_AUDIT_ENV_OTHER";
    for (before, after, reuse) in [
        (vec![FLAG], vec![FLAG, other], false),
        (vec![FLAG, other], vec![FLAG], false),
        (vec![FLAG], vec![other], false),
        (vec![], vec![FLAG], false),
        (vec![FLAG], vec![], false),
        (vec![other, FLAG, other], vec![FLAG, other, FLAG], true),
    ] {
        let fixture = Fixture::new();
        let first = fixture
            .run("session.sqlite", false, &before, Some(OsStr::new("1")))
            .await;
        let resumed = fixture
            .run("session.sqlite", true, &after, Some(OsStr::new("1")))
            .await;
        assert_eq!(
            first.report["run"]["run_id"] == resumed.report["run"]["run_id"],
            reuse
        );
        assert_eq!(resumed.metrics["executed"], usize::from(!reuse));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn invalid_utf8_values_are_distinct_through_the_public_cli() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = Fixture::new();
    let first_value = std::ffi::OsString::from_vec(vec![0x80]);
    let second_value = std::ffi::OsString::from_vec(vec![0x81]);
    let first = fixture
        .run("session.sqlite", false, &[FLAG], Some(&first_value))
        .await;
    let resumed = fixture
        .run("session.sqlite", true, &[FLAG], Some(&second_value))
        .await;
    assert_ne!(
        first.report["run"]["run_id"],
        resumed.report["run"]["run_id"]
    );
    assert_eq!(resumed.metrics["executed"], 1);
}

#[tokio::test]
async fn captured_plaintext_is_absent_from_run_and_session_artifacts() {
    let secret = "hoimin-environment-private-marker-628-unique";
    let fixture = Fixture::new();
    let run = fixture
        .run("session.sqlite", false, &[FLAG], Some(OsStr::new(secret)))
        .await;
    assert!(!run.report.to_string().contains(secret));
    assert_eq!(
        run.report["run"]["normalized_config"]["fingerprint_env"],
        serde_json::json!([FLAG])
    );
    let hash = run.report["run"]["normalized_config"]["fingerprint_env_hash"]
        .as_str()
        .unwrap();
    assert_eq!(hash.len(), 64);
    for path in std::fs::read_dir(fixture.directory.path()).unwrap() {
        let path = path.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(secret.len())
                    .any(|part| part == secret.as_bytes()),
                "{}",
                path.display()
            );
        }
    }
}
