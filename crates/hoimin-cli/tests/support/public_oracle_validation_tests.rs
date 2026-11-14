use super::*;

#[test]
fn fixture_reports_tempdir_failure() {
    let result =
        public_oracle_fixture::create(Err(std::io::Error::other("denied")), "case", "pass");

    assert!(result.unwrap_err().contains("case fixture tempdir: denied"));
}

#[test]
fn fixture_reports_root_failure() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("project"), "obstruction").unwrap();

    let result = public_oracle_fixture::create(Ok(directory), "case", "pass");

    assert!(result.unwrap_err().contains("case fixture root:"));
}

#[test]
fn fixture_reports_source_failure() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("project/target.py")).unwrap();

    let result = public_oracle_fixture::create(Ok(directory), "case", "pass");

    assert!(result.unwrap_err().contains("case fixture source:"));
}

#[test]
fn selector_defaults_to_all_cases() {
    let cases = strict_cases().unwrap();
    let expected = cases.len();

    let result = selected_cases_from(cases, None);

    assert_eq!(result.unwrap().len(), expected);
}

#[test]
fn selector_selects_one_identity() {
    let cases = strict_cases().unwrap();
    let id = cases[0].id.clone();

    let result = selected_cases_from(cases, Some(id.into()));

    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn selector_rejects_missing_identity() {
    let cases = strict_cases().unwrap();

    let result = selected_cases_from(cases, Some("missing".into()));

    assert!(result.unwrap_err().contains("selected 0 strict rows"));
}

#[test]
fn selector_rejects_duplicate_identity() {
    let cases = strict_cases().unwrap();
    let id = cases[0].id.clone();
    let duplicate = vec![cases[0].clone(), cases[0].clone()];

    let result = selected_cases_from(duplicate, Some(id.into()));

    assert!(result.unwrap_err().contains("selected 2 strict rows"));
}

#[cfg(unix)]
#[test]
fn selector_rejects_non_unicode_identity() {
    use std::os::unix::ffi::OsStringExt;
    let cases = strict_cases().unwrap();
    let id = OsString::from_vec(vec![255]);

    let result = selected_cases_from(cases, Some(id));

    assert!(result.unwrap_err().contains("not UTF-8"));
}

#[tokio::test(start_paused = true)]
async fn decoder_reports_timeout() {
    let item = strict_cases().unwrap().remove(0);
    let elapsed =
        tokio::time::timeout(Duration::from_millis(1), std::future::pending::<i32>()).await;

    let result = decode_public_plan(&item, elapsed, b"", b"");

    assert!(result.unwrap_err().contains("public plan timed out"));
}

#[rstest::rstest]
#[case::exit(2, b"".as_slice(), b"failure".as_slice(), "exit=2, stderr=failure")]
#[case::stderr(0, b"".as_slice(), b"warning".as_slice(), "exit=0, stderr=warning")]
#[case::json(0, b"{".as_slice(), b"".as_slice(), "malformed plan")]
fn decoder_rejects_invalid_output(
    #[case] code: i32,
    #[case] stdout: &[u8],
    #[case] stderr: &[u8],
    #[case] expected: &str,
) {
    let item = strict_cases().unwrap().remove(0);

    let result = decode_public_plan(&item, Ok(code), stdout, stderr);

    assert!(result.unwrap_err().contains(expected));
}

async fn strict_fixture() -> (OracleCase, PlanManifest) {
    let mut item = strict_cases().unwrap().remove(0);
    "from typing import Sequence\nvalue: list[int]\n".clone_into(&mut item.source);
    "list[int]".clone_into(&mut item.marker);
    let manifest = public_plan(&item).await.unwrap();
    (item, manifest)
}

#[rstest::rstest]
#[case::matching("list[int]", 1)]
#[case::outside("from typing", 0)]
#[tokio::test]
async fn normalization_counts_candidates_within_marker(
    #[case] marker: &str,
    #[case] expected: usize,
) {
    let (mut item, manifest) = strict_fixture().await;
    marker.clone_into(&mut item.marker);

    let result = normalize_manifest(&item, &manifest);

    assert_eq!(result.unwrap().count, expected);
}

#[tokio::test]
async fn normalization_counts_no_candidates() {
    let (item, mut manifest) = strict_fixture().await;
    manifest.candidates.clear();

    let result = normalize_manifest(&item, &manifest);

    assert_eq!(result.unwrap().count, 0);
}

#[rstest::rstest]
#[case::overlap(|manifest: &mut PlanManifest| manifest.candidates.push(manifest.candidates[0].clone()), "overlaps 2 candidates")]
#[case::overflow(|manifest: &mut PlanManifest| { manifest.candidates[0].candidate.span.start = u64::MAX; manifest.candidates[0].candidate.span.length = 1; }, "span end overflow")]
#[case::text(|manifest: &mut PlanManifest| "wrong".clone_into(&mut manifest.candidates[0].candidate.original), "candidate span disagrees")]
#[tokio::test]
async fn normalization_rejects_invalid_candidate(
    #[case] damage: fn(&mut PlanManifest),
    #[case] expected: &str,
) {
    let (item, mut manifest) = strict_fixture().await;
    damage(&mut manifest);

    let result = normalize_manifest(&item, &manifest);

    assert!(result.unwrap_err().contains(expected));
}

#[tokio::test]
async fn normalization_rejects_missing_marker() {
    let (mut item, manifest) = strict_fixture().await;
    "missing marker".clone_into(&mut item.marker);

    let result = normalize_manifest(&item, &manifest);

    assert!(result.unwrap_err().contains("marker missing"));
}

#[rstest::rstest]
#[case::json("{")]
#[case::empty("")]
fn parser_rejects_invalid_corpus(#[case] input: &str) {
    let result = strict_cases_from(input);

    assert!(result.is_err());
}

#[test]
fn parser_reports_invalid_json_context() {
    let result = strict_cases_from("{");

    assert!(result.unwrap_err().contains("invalid JSON"));
}

#[test]
fn source_rejects_unknown_identity() {
    let result = strict_source("unknown");

    assert!(result.is_none());
}
