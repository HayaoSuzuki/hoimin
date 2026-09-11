use std::path::{Path, PathBuf};
use std::time::Duration;

fn python() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    })
}

async fn output(mut command: tokio::process::Command) -> std::process::Output {
    command.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("bounded child execution")
        .expect("child starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

async fn check(source: &str, candidates: usize) {
    check_operator(source, candidates, "collection_any_all", "all").await;
}

async fn check_operator(source: &str, candidates: usize, operator: &str, replacement: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("subject.py");
    std::fs::write(&path, source).unwrap();
    let mut runtime = tokio::process::Command::new(python());
    runtime.arg(&path).env("PYTHONDONTWRITEBYTECODE", "1");
    output(runtime).await;
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["plan", "--root"])
        .arg(directory.path())
        .args([
            "--file",
            "subject.py",
            "--operators",
            operator,
            "--allow-best-effort-memory",
            "--",
        ])
        .arg(python())
        .args(["-c", "pass"]);
    let result = output(command).await;
    let manifest: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let actual = manifest["candidates"].as_array().unwrap();
    assert_eq!(actual.len(), candidates, "source:\n{source}\n{manifest}");
    for candidate in actual {
        assert!(!candidate["id"].as_str().unwrap().is_empty());
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(
            &source[start..start + length],
            candidate["original"].as_str().unwrap()
        );
        assert_eq!(candidate["operator"], operator);
        assert_eq!(candidate["replacement"], replacement);
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

#[tokio::test]
async fn module_comprehension_named_targets_shadow_source() {
    module_cases("any").await;
}

#[tokio::test]
async fn module_comprehension_named_targets_shadow_destination() {
    module_cases("all").await;
}

async fn module_cases(name: &str) {
    for expression in [
        "[(NAME := custom) for _ in [0]]",
        "{(NAME := custom) for _ in [0]}",
        "{0: (NAME := custom) for _ in [0]}",
        "[[(NAME := custom) for _ in [0]] for _ in [0]]",
        "list((NAME := custom) for _ in [0])",
    ] {
        let expression = expression.replace("NAME", name);
        check(&format!("custom = lambda values: 'custom'\n{expression}\nassert {name} is custom\nresult = any([False, True])\n"), 0).await;
    }
}

#[tokio::test]
async fn comprehension_controls_preserve_real_candidates() {
    for prefix in [
        "[any for any in [0]]",
        "[(unrelated := 0) for _ in [0]]",
        "[(lambda: (any := 0))() for _ in [0]]",
        "def unrelated():\n    [(any := 0) for _ in [0]]",
    ] {
        check(&format!("{prefix}\nassert any([False, True]) is True\n"), 1).await;
    }
}

#[tokio::test]
async fn empty_and_lazy_comprehensions_are_possible_module_writes() {
    for expression in [
        "[(any := custom) for _ in []]",
        "((any := custom) for _ in [0])",
    ] {
        check(&format!("import builtins\ncustom = lambda values: 'custom'\n{expression}\nassert any is builtins.any\nresult = any([False, True])\n"), 0).await;
    }
}

#[tokio::test]
async fn named_comprehensions_declare_static_function_locals() {
    for expression in ["[(any := 0) for _ in []]", "((any := 0) for _ in [0])"] {
        check(&format!("def f():\n    result = any([False, True])\n    {expression}\ntry:\n    f()\nexcept UnboundLocalError:\n    pass\nelse:\n    raise AssertionError('walrus must declare function local')\n"), 0).await;
    }
}

#[tokio::test]
async fn containing_global_and_nonlocal_directives_apply() {
    check("custom = lambda values: 'custom'\ndef f():\n    global any\n    [(any := custom) for _ in [0]]\nf()\nassert any is custom\nresult = any([False, True])\n", 0).await;
    check("def outer():\n    any = lambda values: 'custom'\n    def inner():\n        nonlocal any\n        [(any := lambda values: 'updated') for _ in [0]]\n        return any([False, True])\n    assert inner() == 'updated'\nouter()\n", 0).await;
}

#[tokio::test]
async fn delayed_generator_write_survives_intervening_reset() {
    for reset in [
        "any = builtins.any",
        "from builtins import any",
        "any = 0\ndel any",
    ] {
        check(&format!("import builtins\ncustom = lambda values: 'custom'\ngenerator = ((any := custom) for _ in [0])\n{reset}\nnext(generator)\nassert any is custom\nresult = any([False, True])\n"), 0).await;
    }
}

#[tokio::test]
async fn rhs_keeps_comprehension_scope_and_outer_loop_backedges() {
    check("custom = lambda values: True\n[(unrelated := any([False, True])) for any in [custom]]\nassert any([False, True]) is True\n", 1).await;
    check("custom = lambda values: True\nfor _ in [0, 1]:\n    result = any([False, True])\n    [(any := custom) for _ in [0]]\nassert any is custom\n", 0).await;
    check(
        "assert any([False, True]) is True\n[(any := lambda values: True) for _ in [0]]\n",
        1,
    )
    .await;
}

#[tokio::test]
async fn false_kill_regression_runs_with_passing_baseline() {
    let directory = tempfile::tempdir().unwrap();
    let source =
        "[(any := lambda values: 'custom') for _ in [0]]\nassert any([False, True]) == 'custom'\n";
    let path = directory.path().join("subject.py");
    std::fs::write(&path, source).unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["run", "--root"])
        .arg(directory.path())
        .args([
            "--file",
            "subject.py",
            "--operators",
            "collection_any_all",
            "--format",
            "json",
            "--min-free-space",
            "1B",
            "--allow-best-effort-memory",
            "--",
        ])
        .arg(python())
        .args(["subject.py"]);
    let result = output(command).await;
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["summary"]["counts"]["killed"], 0, "{report}");
    assert!(report["mutants"].as_array().unwrap().is_empty(), "{report}");
    assert_eq!(report["baseline"]["termination"]["Exit"], 0, "{report}");
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LeanCase {
    schema: u64,
    mode: String,
    id: String,
    source: String,
    owner: u64,
    expected_present: bool,
}

#[tokio::test]
async fn lean_named_binding_routing_matches_public_plan_and_cpython() {
    let corpus = include_str!("../../../formal/HoiminOracle/corpus/comprehension-bindings.jsonl");
    let mut ids = std::collections::BTreeSet::new();
    let mut positives = 0;
    for line in corpus.lines() {
        let case: LeanCase = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "strict");
        assert!(ids.insert(case.id));
        assert!(case.owner <= 3);
        positives += usize::from(case.expected_present);
        check(&case.source, usize::from(case.expected_present)).await;
    }
    assert_eq!(
        ids.len(),
        11,
        "all bounded model scenarios must be exercised"
    );
    assert_eq!(
        positives, 3,
        "lambda, function and iteration controls remain eligible"
    );
}

#[tokio::test]
async fn named_binding_routes_apply_to_other_builtin_pairs() {
    for (original, replacement, operator) in [
        ("list", "tuple", "collection_list_tuple"),
        ("set", "frozenset", "collection_set_frozenset"),
        ("min", "max", "collection_min_max"),
        ("sorted", "reversed", "structure_sorted_reversed"),
    ] {
        for name in [original, replacement] {
            check_operator(&format!("custom = lambda values: 0\n[({name} := custom) for _ in range(1)]\nassert {name} is custom\nresult = {original}(range(2))\n"), 0, operator, replacement).await;
        }
        check_operator(
            &format!("[(unrelated := 0) for _ in range(1)]\nresult = {original}(range(2))\n"),
            1,
            operator,
            replacement,
        )
        .await;
    }
}
