use super::*;

#[rstest::rstest]
#[case::json("{", "invalid JSON")]
#[case::object("[]", "not an object")]
#[case::empty("", "no cases")]
fn parser_rejects_invalid_corpus(#[case] input: &str, #[case] expected: &str) {
    let result = parse_corpus(input);

    assert!(result.unwrap_err().contains(expected));
}

#[test]
fn parser_rejects_unknown_schema() {
    let row = CORPUS.lines().find(|line| !line.is_empty()).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(row).unwrap();
    value["schema"] = serde_json::json!("wrong");

    let result = parse_corpus(&value.to_string());

    assert!(result.unwrap_err().contains("violates the schema"));
}

#[test]
fn parser_ignores_empty_lines() {
    let expected = parse_corpus(CORPUS).unwrap().len();

    let result = parse_corpus(&format!("\n{CORPUS}"));

    assert_eq!(result.unwrap().len(), expected);
}

#[test]
fn validator_rejects_unknown_identity() {
    let mut item = parse_corpus(CORPUS).unwrap().remove(0);
    item.id = "unknown".into();

    let result = validate_case(&item);

    assert!(result.unwrap_err().contains("not a recognized"));
}

#[test]
fn pair_contract_rejects_unknown_scenario() {
    let result = pair_scenario_contract("unknown");

    assert!(result.is_none());
}

#[test]
fn normalized_states_reject_unknown_state() {
    let mut item = parse_corpus(CORPUS).unwrap().remove(0);
    item.expected_fallthrough = vec![vec!["unknown".into()]];

    let result = validate_normalized_states(&item);

    assert!(result.unwrap_err().contains("unknown normalized"));
}

#[test]
fn strict_state_rejects_internal_loop_head() {
    let mut item = parse_corpus(CORPUS).unwrap().remove(0);
    item.expected_fallthrough.clear();
    item.mode = "strict".into();
    item.expected_loop_head = Some(vec![]);

    let result = validate_normalized_states(&item);

    assert!(result.unwrap_err().contains("outside an internal"));
}

#[test]
fn fixture_reports_tempdir_failure() {
    let directory = Err(std::io::Error::other("denied"));

    let result = Fixture::from_tempdir("pass", directory);

    assert_eq!(result.err().unwrap(), "denied");
}

#[rstest::rstest]
#[case::root("project", false)]
#[case::source("project/src/case.py", true)]
#[case::metadata("project/pyproject.toml", true)]
fn fixture_reports_filesystem_failure(#[case] obstruction: &str, #[case] is_directory: bool) {
    let directory = tempfile::tempdir().unwrap();
    create_obstruction(directory.path().join(obstruction), is_directory);

    let result = Fixture::from_tempdir("pass", Ok(directory));

    assert!(result.is_err());
}

fn create_obstruction(path: std::path::PathBuf, is_directory: bool) {
    if is_directory {
        std::fs::create_dir_all(path).unwrap();
    } else {
        std::fs::write(path, "blocked").unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn decoder_reports_deadline() {
    let item = parse_corpus(CORPUS).unwrap().remove(0);
    let elapsed =
        tokio::time::timeout(Duration::from_millis(1), std::future::pending::<i32>()).await;

    let result = decode_public_plan(&item, elapsed, b"", b"");

    assert!(result.unwrap_err().contains("timed out"));
}

#[rstest::rstest]
#[case::exit(2, b"".as_slice(), b"failed".as_slice(), "exit=2, stderr=failed")]
#[case::stderr(0, b"".as_slice(), b"warning".as_slice(), "exit=0, stderr=warning")]
#[case::json(0, b"{".as_slice(), b"".as_slice(), "malformed plan")]
fn decoder_rejects_invalid_payload(
    #[case] code: i32,
    #[case] stdout: &[u8],
    #[case] stderr: &[u8],
    #[case] expected: &str,
) {
    let item = parse_corpus(CORPUS).unwrap().remove(0);

    let result = decode_public_plan(&item, Ok(code), stdout, stderr);

    assert!(result.unwrap_err().contains(expected));
}

async fn strict_fixture() -> (OracleCase, PlanManifest) {
    let item = parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .find(|item| item.mode == "strict" && item.expected_present)
        .unwrap();
    let manifest = public_plan(&item).await.unwrap();
    (item, manifest)
}

#[tokio::test]
async fn observation_rejects_overlapping_candidates() {
    let (item, mut manifest) = strict_fixture().await;
    manifest.candidates.push(manifest.candidates[0].clone());

    let result = observe_manifest(&item, &manifest);

    assert!(result.unwrap_err().contains("matching candidates"));
}

#[tokio::test]
async fn observation_ignores_overflowing_span() {
    let (item, mut manifest) = strict_fixture().await;
    manifest.candidates[0].candidate.span.start = u64::MAX;
    manifest.candidates[0].candidate.span.length = 1;

    let result = observe_manifest(&item, &manifest);

    assert!(!result.unwrap().present);
}

#[tokio::test]
async fn observation_rejects_missing_marker() {
    let (mut item, manifest) = strict_fixture().await;
    item.site_marker = "missing".into();

    let result = observe_manifest(&item, &manifest);

    assert!(result.unwrap_err().contains("marker occurs 0 times"));
}
