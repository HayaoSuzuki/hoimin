use std::ffi::OsString;
use std::path::Path;

use hoimin_cli::plan::PlanManifest;
use hoimin_core::MutationOperatorSelection;

const PROMOTED: [&str; 7] = [
    "statement_delete",
    "integer_literal_neighbor",
    "augmented_to_assignment",
    "return_tuple_swap",
    "string_literal_empty",
    "while_condition_false",
    "conversion_call_remove",
];

async fn cli(args: Vec<OsString>) -> serde_json::Value {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert!(
        matches!(code, 0 | 1),
        "exit={code}: {}",
        String::from_utf8_lossy(&stderr)
    );
    serde_json::from_slice(&stdout).unwrap()
}

async fn plan(root: &Path, selection: &[&str]) -> serde_json::Value {
    let python = std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.venv")
            .join(if cfg!(windows) {
                "Scripts/python.exe"
            } else {
                "bin/python"
            })
            .into_os_string()
    });
    let mut args: Vec<OsString> = vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        root.as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--allow-best-effort-memory".into(),
    ];
    args.extend(selection.iter().map(OsString::from));
    args.extend([
        "--".into(),
        python,
        "-B".into(),
        "-c".into(),
        "import subject".into(),
    ]);
    cli(args).await
}

#[tokio::test]
async fn default_candidates_are_opt_out_and_saved_legacy_plans_stay_legacy() {
    let root = tempfile::tempdir().unwrap();
    let source = "def f(x, y):\n    print('message')\n    x += 3\n    while x < y:\n        x += 1\n    z = int(x)\n    return x, y\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let default = plan(root.path(), &[]).await;
    let manifest: PlanManifest = serde_json::from_value(default.clone()).unwrap();
    assert_eq!(manifest.normalized_config.operators.names().len(), 50);
    let candidates = default["candidates"].as_array().unwrap();
    for operator in PROMOTED {
        assert!(
            candidates.iter().any(|c| c["operator"] == operator),
            "missing {operator}"
        );
        let excluded = plan(root.path(), &["--exclude-operators", operator]).await;
        let expected: Vec<_> = candidates
            .iter()
            .filter(|c| c["operator"] != operator)
            .cloned()
            .collect();
        // Ranks and discovery sequences are renumbered after exclusion; the
        // ordered candidate identities and descriptors must remain the same.
        let descriptors = |rows: &[serde_json::Value]| {
            rows.iter()
                .cloned()
                .map(|mut c| {
                    c.as_object_mut().unwrap().remove("rank");
                    c.as_object_mut().unwrap().remove("sequence");
                    c
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            descriptors(excluded["candidates"].as_array().unwrap()),
            descriptors(&expected)
        );
    }
    let names = manifest.normalized_config.operators.names().join(",");
    let explicit = plan(root.path(), &["--operators", &names]).await;
    assert_eq!(explicit["candidates"], default["candidates"]);
    let legacy = plan(
        root.path(),
        &[
            "--operators",
            &MutationOperatorSelection::all_legacy().names().join(","),
        ],
    )
    .await;
    let excluded = plan(root.path(), &["--exclude-operators", &PROMOTED.join(",")]).await;
    assert_eq!(excluded["candidates"], legacy["candidates"]);
    assert!(legacy["candidates"].as_array().unwrap().len() < candidates.len());
    let only_strings = plan(root.path(), &["--operators", "string_literal_empty"]).await;
    assert_eq!(only_strings["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(
        only_strings["candidates"][0]["operator"],
        "string_literal_empty"
    );

    // Simulate a pre-promotion plan: explicit serialized legacy selection, no new IDs.
    let path = root.path().join("legacy-plan.json");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let report = cli(vec![
        "hoimin".into(),
        "verify".into(),
        path.into_os_string(),
        "--top".into(),
        "1".into(),
        "--format".into(),
        "json".into(),
    ])
    .await;
    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["counts"]["survived"], 1);
    assert_eq!(
        std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
        source
    );
}
