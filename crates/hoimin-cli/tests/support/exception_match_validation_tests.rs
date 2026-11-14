use super::*;

#[rstest::rstest]
#[case::json("{", "invalid JSON")]
#[case::object("[]", "not an object")]
#[case::empty("", "no cases")]
#[case::incomplete(CORPUS.lines().find(|line| !line.is_empty()).unwrap(), "exact schema-owned")]
fn parser_rejects_invalid_corpus(#[case] input: &str, #[case] expected: &str) {
    let result = parse_corpus(input);

    assert!(result.unwrap_err().contains(expected));
}

#[test]
fn parser_ignores_empty_lines() {
    let result = parse_corpus(&format!("\n{CORPUS}"));

    assert_eq!(result.unwrap().len(), EXPECTED_CASE_KEYS.len());
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
fn normalized_facts_reject_duplicates() {
    let facts = [
        "direct:Sequence=typing.Sequence".into(),
        "direct:Sequence=typing.Sequence".into(),
    ];

    let result = facts_are_normalized(&facts);

    assert!(!result);
}

#[rstest::rstest]
#[case::identity(|case: &mut OracleCase| case.id.clear(), "invalid identity")]
#[case::facts(|case: &mut OracleCase| case.expected_facts = vec!["unknown".into()], "invalid normalized facts")]
fn validator_rejects_invalid_contract(#[case] damage: fn(&mut OracleCase), #[case] expected: &str) {
    let mut case = parse_corpus(CORPUS).unwrap().remove(0);
    damage(&mut case);

    let result = validate_case(&case);

    assert!(result.unwrap_err().contains(expected));
}

#[test]
fn fields_reject_unknown_observation_kind() {
    let mut case = parse_corpus(CORPUS).unwrap().remove(0);
    case.observation_kind = "unknown".into();

    let result = fields_match(&case);

    assert!(!result);
}

#[test]
fn fixture_reports_tempdir_failure() {
    let item = parse_corpus(CORPUS).unwrap().remove(0);

    let result = Fixture::from_tempdir(&item, Err(std::io::Error::other("denied")));

    assert!(result.err().unwrap().contains("fixture tempdir: denied"));
}

#[rstest::rstest]
#[case::source("src/case.py", "fixture write source")]
#[case::metadata("pyproject.toml", "fixture write project metadata")]
fn fixture_preserves_failure_stage(#[case] obstruction: &str, #[case] expected: &str) {
    let item = parse_corpus(CORPUS).unwrap().remove(0);
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join(obstruction)).unwrap();

    let result = write_fixture_files(&item, directory.path());

    assert!(result.unwrap_err().contains(expected));
}

#[test]
fn wait_reports_io_error() {
    let result = decode_wait("case", Ok(Err(std::io::Error::other("reaped"))));

    assert!(result.unwrap_err().contains("wait failed: reaped"));
}

#[cfg(unix)]
#[rstest::rstest]
#[case::success(0, |result| matches!(result, ProcessTermination::Success))]
#[case::exit(256, |result| matches!(result, ProcessTermination::Exit(1)))]
#[case::signal(9, |result| matches!(result, ProcessTermination::Signal { signal: 9, .. }))]
#[case::abnormal(127, |result| matches!(result, ProcessTermination::Abnormal))]
fn termination_decodes_platform_state(
    #[case] raw: i32,
    #[case] expected: fn(ProcessTermination) -> bool,
) {
    use std::os::unix::process::ExitStatusExt;

    let result = process_termination(ExitStatus::from_raw(raw));

    assert!(expected(result));
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
async fn normalization_excludes_candidate_outside_marker() {
    let (mut item, manifest) = strict_fixture().await;
    item.marker =
        item.source[..usize::try_from(manifest.candidates[0].span.start).unwrap()].to_owned();

    let result = normalize_manifest(&item, &manifest);

    assert_eq!(result.unwrap().count, 0);
}

#[rstest::rstest]
#[case::overflow(|manifest: &mut PlanManifest| { manifest.candidates[0].candidate.span.start = u64::MAX; manifest.candidates[0].candidate.span.length = 1; }, "candidate range overflow")]
#[case::text(|manifest: &mut PlanManifest| manifest.candidates[0].candidate.original = "wrong".into(), "span does not match")]
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
    item.marker = "missing".into();

    let result = normalize_manifest(&item, &manifest);

    assert!(result.unwrap_err().contains("marker occurs 0 times"));
}

#[tokio::test]
async fn public_plan_requires_operator() {
    let mut item = parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .find(|item| item.mode == "strict" && item.expected_present)
        .unwrap();
    item.operator = None;

    let result = public_plan(&item).await;

    assert!(result.unwrap_err().contains("operator missing"));
}
