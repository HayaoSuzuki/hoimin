#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;

use std::ffi::OsString;
use std::path::Path;

use hoimin_cli::plan::PlanManifest;

const OPT_OUT_OPERATORS: [&str; 26] = [
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
    "stdlib_conversion_call_remove",
    "loop_break_insert",
    "iterator_stop_insert",
    "async_iterator_stop_insert",
    "list_comprehension_to_generator",
    "generator_return_insert",
    "generator_skip_first",
    "generator_empty",
    "none_truthiness",
    "mutable_default_share",
    "nonlocal_remove",
    "yield_from_to_yield",
    "mapping_setdefault_to_get",
    "bisect_boundary_swap",
    "deepcopy_to_copy",
    "deque_popleft_to_pop",
];

async fn cli(args: Vec<OsString>) -> serde_json::Value {
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;
    assert!(
        code == 0,
        "exit={code}: {}",
        String::from_utf8_lossy(&stderr.snapshot())
    );
    jsonl_report::from_slice(&stdout.snapshot()).unwrap()
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
async fn current_default_candidates_are_individually_excludable() {
    let root = tempfile::tempdir().unwrap();
    let source = "from decimal import Decimal\nfrom http import HTTPStatus\ndef amount(x): return Decimal(x)\ndef factory(x): return Decimal.from_float(x)\ndef status(x): return HTTPStatus(x)\ndef f(x, y):\n    print('message')\n    x += 3\n    while x < y:\n        x += 1\n    z = int(x)\n    return x, y\ndef recent(text) -> str:\n    return text.strip()\ndef segment(x):\n    return f'prefix:{x}'\n";
    let source = format!(
        "{source}\nclass Items:\n    def __next__(self): return 1\n    async def __anext__(self): return 2\n"
    );
    let source = format!(
        "{source}\ndef produced(): yield 1\neager = [x for x in (1, 2)]\nlazy = (x for x in (1, 2))\n"
    );
    let source = format!(
        "{source}\ndef owned(values=None):\n    if values is None: values=[]\n    def inner():\n        nonlocal values\n        values=[1]\n    yield from values\nstate={{}}\nstate.setdefault('x', [])\n"
    );
    let source = format!(
        "{source}\nimport bisect\nimport copy\nfrom collections import deque\nposition=bisect.bisect_left([1,1],1)\ncloned=copy.deepcopy([[1]])\nqueue=deque([1,2])\nfirst=queue.popleft()\n"
    );
    std::fs::write(root.path().join("subject.py"), &source).unwrap();
    let default = plan(root.path(), &[]).await;
    let manifest: PlanManifest = serde_json::from_value(default.clone()).unwrap();
    assert_eq!(manifest.normalized_config.operators.names().len(), 69);
    let candidates = default["candidates"].as_array().unwrap();
    for original in ["Decimal(x)", "Decimal.from_float(x)", "HTTPStatus(x)"] {
        assert!(
            candidates
                .iter()
                .any(|c| c["operator"] == "stdlib_conversion_call_remove"
                    && c["original"] == original)
        );
    }
    for operator in OPT_OUT_OPERATORS {
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
    assert_eq!(
        std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
        source
    );
}

#[tokio::test]
async fn explicit_subset_exclusions_do_not_add_default_candidates() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), "value = left == 'text'\n").unwrap();

    let selected = plan(
        root.path(),
        &[
            "--operators",
            "compare_eq_ne,string_literal_empty",
            "--exclude-operators",
            "string_literal_empty",
        ],
    )
    .await;

    assert_eq!(
        selected["normalized_config"]["operators"],
        serde_json::json!(["compare_eq_ne"])
    );
    let candidates = selected["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["operator"], "compare_eq_ne");
    assert_eq!(candidates[0]["original"], "==");
    assert_eq!(candidates[0]["replacement"], "!=");
}

#[tokio::test]
async fn saved_explicit_subset_prepares_and_verifies_without_expansion() {
    let root = tempfile::tempdir().unwrap();
    let source = "def clean(text) -> str:\n    return text.strip() + \"!\"\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let selected = plan(
        root.path(),
        &["--operators", "method_call_remove,binary_add_sub"],
    )
    .await;
    assert_eq!(
        selected["normalized_config"]["operators"],
        serde_json::json!(["binary_add_sub", "method_call_remove"])
    );
    assert_eq!(selected["candidates"].as_array().unwrap().len(), 2);
    let path = root.path().join("selected-plan.json");
    std::fs::write(&path, serde_json::to_vec(&selected).unwrap()).unwrap();

    let prepared = hoimin_cli::plan::prepare_verify(
        &path,
        &[selected["candidates"][0]["id"].as_str().unwrap().to_owned()],
    )
    .await
    .unwrap();

    assert_eq!(
        prepared.config().operators.names(),
        ["binary_add_sub", "method_call_remove"]
    );
    let report = cli(vec![
        "hoimin".into(),
        "verify".into(),
        path.into_os_string(),
        "--top".into(),
        "1".into(),
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
