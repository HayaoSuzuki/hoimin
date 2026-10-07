use std::ffi::OsString;
use std::path::{Path, PathBuf};

async fn cli(args: Vec<OsString>) -> (i32, serde_json::Value, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut out, &mut err).await;
    (
        code,
        serde_json::from_slice(&out).unwrap_or(serde_json::Value::Null),
        String::from_utf8(err).unwrap(),
    )
}

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.venv")
                .join(if cfg!(windows) {
                    "Scripts/python.exe"
                } else {
                    "bin/python"
                })
        },
        PathBuf::from,
    )
}

async fn fixture() -> (tempfile::TempDir, PathBuf, serde_json::Value) {
    fixture_with_budget(20).await
}

async fn fixture_with_budget(
    max_mutants: usize,
) -> (tempfile::TempDir, PathBuf, serde_json::Value) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("a.py"),
        "a = True; b = False; c = True\nd = False\nx = 1 + 2; y = 3 + 4\n",
    )
    .unwrap();
    std::fs::write(root.join("b.py"), "a = True\nx = 1 + 2\n").unwrap();
    let (code, document, err) = cli(vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        root.into_os_string(),
        "--source".into(),
        ".".into(),
        "--operators".into(),
        "boolean_literal,binary_add_sub".into(),
        "--max-mutants".into(),
        max_mutants.to_string().into(),
        "--jobs".into(),
        "1".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python().into_os_string(),
        "-B".into(),
        "-c".into(),
        "import a, b".into(),
    ])
    .await;
    assert_eq!(code, 0, "{err}");
    assert_eq!(document["candidates"].as_array().unwrap().len(), 8);
    let path = directory.path().join("plan.json");
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    (directory, path, document)
}

async fn preview(path: &Path, offset: usize, count: usize) -> serde_json::Value {
    let (code, output, err) = cli(vec![
        "hoimin".into(),
        "verify".into(),
        path.as_os_str().to_owned(),
        "--top".into(),
        count.to_string().into(),
        "--offset".into(),
        offset.to_string().into(),
        "--selection-policy".into(),
        "line-diverse".into(),
        "--dry-run".into(),
        "--format".into(),
        "json".into(),
    ])
    .await;
    assert_eq!(code, 0, "{err}");
    output
}

#[tokio::test]
async fn line_diverse_public_cli_orders_then_pages_equal_score_lines() {
    let (_directory, path, plan) = fixture().await;
    let before = std::fs::read(&path).unwrap();
    let output = preview(&path, 0, 20).await;
    let selected = output["candidates"].as_array().unwrap();
    let ranked = plan["candidates"].as_array().unwrap();
    let expected = [0, 3, 4, 1, 2, 5, 7, 6];
    assert_eq!(
        selected.iter().map(|c| &c["id"]).collect::<Vec<_>>(),
        expected.map(|i| &ranked[i]["id"])
    );
    assert_eq!(
        output["verification_selection"]["policy"],
        "line_round_robin_v1"
    );
    let mut pages = Vec::new();
    for offset in [0, 3, 6] {
        let page = preview(&path, offset, 3).await;
        pages.extend(
            page["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["id"].clone()),
        );
    }
    assert_eq!(
        pages,
        selected.iter().map(|c| c["id"].clone()).collect::<Vec<_>>()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[tokio::test]
async fn line_diverse_execution_matches_preview_order_and_preserves_report_scope() {
    let (_directory, path, _plan) = fixture().await;
    let preview = preview(&path, 1, 3).await;
    let (code, report, err) = cli(vec![
        "hoimin".into(),
        "verify".into(),
        path.as_os_str().to_owned(),
        "--top".into(),
        "3".into(),
        "--offset".into(),
        "1".into(),
        "--selection-policy".into(),
        "line-diverse".into(),
        "--format".into(),
        "json".into(),
    ])
    .await;
    assert_eq!(code, 1, "{err}");
    assert_eq!(
        report["run"]["verification_selection"],
        preview["verification_selection"]
    );
    assert_eq!(
        report["summary"]["verification_selection"],
        preview["verification_selection"]
    );
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(
        report["run"]["verification_selection"]["scope"],
        "retained_candidates"
    );
    let expected = preview["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["id"])
        .collect::<Vec<_>>();
    let actual = report["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| &m["candidate"]["id"])
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    let _: hoimin_core::VerificationSelection =
        serde_json::from_value(report["run"]["verification_selection"].clone()).unwrap();
    let report_path = path.with_file_name("report.json");
    std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let (code, progress, err) = cli(vec![
        "hoimin".into(),
        "progress".into(),
        report_path.as_os_str().to_owned(),
        report_path.as_os_str().to_owned(),
        "--format".into(),
        "json".into(),
    ])
    .await;
    assert_eq!(code, 0, "{err}");
    assert_eq!(progress["comparisons"][0]["common"], 3);
    assert_eq!(progress["comparisons"][0]["added"], 0);
    assert_eq!(progress["comparisons"][0]["removed"], 0);
}

#[tokio::test]
async fn line_diverse_requires_top_and_preserves_mutant_budget() {
    let (_directory, path, plan) = fixture_with_budget(2).await;
    for options in [
        vec!["--selection-policy", "line-diverse", "--dry-run"],
        vec![
            "--selection-policy",
            "line-diverse",
            "--candidate",
            plan["candidates"][0]["id"].as_str().unwrap(),
            "--dry-run",
        ],
        vec![
            "--selection-policy",
            "line-diverse",
            "--top",
            "3",
            "--dry-run",
        ],
    ] {
        let mut args = vec![
            "hoimin".into(),
            "verify".into(),
            path.as_os_str().to_owned(),
        ];
        args.extend(options.into_iter().map(OsString::from));
        let (code, output, err) = cli(args).await;
        assert_eq!(code, 2, "{err}");
        assert!(output.is_null());
        assert_ne!(err, "");
    }
}
