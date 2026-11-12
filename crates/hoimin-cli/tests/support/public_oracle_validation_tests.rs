use super::*;

#[test]
fn fixture_reports_each_filesystem_boundary() {
    let error = public_oracle_fixture::create(Err(std::io::Error::other("denied")), "case", "pass")
        .unwrap_err();
    assert!(error.contains("case fixture tempdir: denied"));
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("project"), "obstruction").unwrap();
    assert!(
        public_oracle_fixture::create(Ok(directory), "case", "pass")
            .unwrap_err()
            .contains("case fixture root:")
    );
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("project/target.py")).unwrap();
    assert!(
        public_oracle_fixture::create(Ok(directory), "case", "pass")
            .unwrap_err()
            .contains("case fixture source:")
    );
}

#[test]
fn selector_checks_identity_and_unicode() {
    let cases = strict_cases().unwrap();
    let id = cases[0].id.clone();
    assert_eq!(
        selected_cases_from(cases.clone(), None).unwrap().len(),
        cases.len()
    );
    assert_eq!(
        selected_cases_from(cases.clone(), Some(id.clone().into()))
            .unwrap()
            .len(),
        1
    );
    assert!(
        selected_cases_from(cases.clone(), Some("missing".into()))
            .unwrap_err()
            .contains("selected 0 strict rows")
    );
    assert!(
        selected_cases_from(vec![cases[0].clone(), cases[0].clone()], Some(id.into()))
            .unwrap_err()
            .contains("selected 2 strict rows")
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        assert!(
            selected_cases_from(cases, Some(OsString::from_vec(vec![255])))
                .unwrap_err()
                .contains("not UTF-8")
        );
    }
}

#[tokio::test(start_paused = true)]
async fn decoder_distinguishes_timeout_exit_stderr_and_bad_json() {
    let item = strict_cases().unwrap().remove(0);
    let elapsed =
        tokio::time::timeout(Duration::from_millis(1), std::future::pending::<i32>()).await;
    assert!(
        decode_public_plan(&item, elapsed, b"", b"")
            .unwrap_err()
            .contains("public plan timed out")
    );
    assert!(
        decode_public_plan(&item, Ok(2), b"", b"failure")
            .unwrap_err()
            .contains("exit=2, stderr=failure")
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
async fn normalization_checks_spans_and_overlapping_candidates() {
    let mut item = strict_cases().unwrap().remove(0);
    item.source = "from typing import Sequence\nvalue: list[int]\n".to_owned();
    item.marker = "list[int]".to_owned();
    let manifest = public_plan(&item).await.unwrap();
    assert_eq!(normalize_manifest(&item, &manifest).unwrap().count, 1);
    item.marker = "from typing".to_owned();
    assert_eq!(normalize_manifest(&item, &manifest).unwrap().count, 0);
    item.marker = "list[int]".to_owned();
    let mut changed = manifest.clone();
    changed.candidates.clear();
    assert_eq!(normalize_manifest(&item, &changed).unwrap().count, 0);
    changed = manifest.clone();
    changed.candidates.push(changed.candidates[0].clone());
    assert!(
        normalize_manifest(&item, &changed)
            .unwrap_err()
            .contains("overlaps 2 candidates")
    );
    changed = manifest.clone();
    changed.candidates[0].candidate.span.start = u64::MAX;
    changed.candidates[0].candidate.span.length = 1;
    assert!(
        normalize_manifest(&item, &changed)
            .unwrap_err()
            .contains("span end overflow")
    );
    changed = manifest.clone();
    changed.candidates[0].candidate.original = "wrong".to_owned();
    assert!(
        normalize_manifest(&item, &changed)
            .unwrap_err()
            .contains("candidate span disagrees")
    );
    item.marker = "missing marker".to_owned();
    assert!(
        normalize_manifest(&item, &manifest)
            .unwrap_err()
            .contains("marker missing")
    );
}

#[test]
fn parser_rejects_malformed_json_and_incomplete_corpus() {
    assert!(strict_cases_from("{").unwrap_err().contains("invalid JSON"));
    assert!(strict_cases_from("").is_err());
    assert!(strict_source("unknown").is_none());
}
