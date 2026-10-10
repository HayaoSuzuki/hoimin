use std::ffi::OsString;
use std::path::Path;

use hoimin_cli::plan::PlanManifest;
use hoimin_core::MutationOperatorSelection;

const PROMOTED: [&str; 10] = [
    "statement_delete",
    "integer_literal_neighbor",
    "augmented_to_assignment",
    "return_tuple_swap",
    "string_literal_empty",
    "while_condition_false",
    "conversion_call_remove",
    "method_call_remove",
    "function_body_return_constant",
    "string_segment_empty",
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
    let source = "def f(x, y):\n    print('message')\n    x += 3\n    while x < y:\n        x += 1\n    z = int(x)\n    return x, y\ndef recent(text) -> str:\n    return text.strip()\ndef segment(x):\n    return f'prefix:{x}'\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let default = plan(root.path(), &[]).await;
    let manifest: PlanManifest = serde_json::from_value(default.clone()).unwrap();
    assert_eq!(manifest.normalized_config.operators.names().len(), 53);
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

#[tokio::test]
async fn excluding_recent_defaults_recovers_and_verifies_previous_fifty_selection() {
    let root = tempfile::tempdir().unwrap();
    let source = "def clean(text) -> str:\n    return text.strip() + \"!\"\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let mut previous = MutationOperatorSelection::all_legacy().names();
    previous.extend(PROMOTED[..7].iter().map(ToString::to_string));
    assert_eq!(previous.len(), 50);
    let old = plan(root.path(), &["--operators", &previous.join(",")]).await;
    let opt_out = plan(
        root.path(),
        &[
            "--exclude-operators",
            "method_call_remove,function_body_return_constant,string_segment_empty",
        ],
    )
    .await;
    assert_eq!(
        old["normalized_config"]["operators"],
        opt_out["normalized_config"]["operators"]
    );
    assert_eq!(old["candidates"], opt_out["candidates"]);
    let current = plan(root.path(), &[]).await;
    for name in ["method_call_remove", "function_body_return_constant"] {
        assert!(
            current["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["operator"] == name)
        );
        assert!(
            !old["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["operator"] == name)
        );
    }
    // Observe the prepared configuration as well as execution compatibility.
    let path = root.path().join("previous-plan.json");
    std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    let prepared = hoimin_cli::plan::prepare_verify(
        &path,
        &[old["candidates"][0]["id"].as_str().unwrap().to_owned()],
        hoimin_cli::cli::OutputFormat::Json,
    )
    .await
    .unwrap();
    let expected: std::collections::BTreeSet<_> = previous.into_iter().collect();
    assert_eq!(
        prepared
            .config
            .operators
            .names()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
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
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(
        std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
        source
    );
}
