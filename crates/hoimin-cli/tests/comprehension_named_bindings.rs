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
    let original = match operator {
        "collection_any_all" => "any",
        "collection_list_tuple" => "list",
        "collection_set_frozenset" => "set",
        "collection_min_max" => "min",
        "structure_sorted_reversed" => "sorted",
        _ => panic!("unsupported test operator: {operator}"),
    };
    // Literal mutations are separate candidates for collection operators.
    let actual: Vec<_> = manifest["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|candidate| candidate["original"] == original)
        .collect();
    assert_eq!(actual.len(), candidates, "source:\n{source}\n{manifest}");
    for candidate in actual {
        let identity = hoimin_core::CandidateIdentity {
            schema_version: hoimin_core::CANDIDATE_SCHEMA_VERSION,
            file_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            path: "subject.py".into(),
            span: serde_json::from_value(candidate["span"].clone()).unwrap(),
            operator: operator.to_owned(),
            replacement: replacement.to_owned(),
        };
        assert_eq!(
            candidate["id"],
            hoimin_core::stable_mutant_id(&identity).as_str()
        );
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(
            &source[start..start + length],
            candidate["original"].as_str().unwrap()
        );
        if source.contains("[any((0, 1))]") {
            assert_eq!(start, source.find("any((0, 1))").unwrap());
            assert_eq!(candidate["original"], "any");
        }
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
        assert_eq!(
            case.source.matches("any(").count(),
            1,
            "one observation per oracle row"
        );
        positives += usize::from(case.expected_present);
        check(&case.source, usize::from(case.expected_present)).await;
    }
    assert_eq!(
        ids.len(),
        18,
        "all bounded model scenarios must be exercised"
    );
    assert_eq!(
        positives, 8,
        "five first-iterable witnesses and three scope controls remain eligible"
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

#[tokio::test]
async fn first_iterable_precedes_body_named_bindings() {
    for expression in [
        "[(NAME := lambda values: \"custom\", item)[1] for item in [any((0, 1))]]",
        "{(NAME := lambda values: \"custom\", item)[1] for item in [any((0, 1))]}",
        "{item: (NAME := lambda values: \"custom\") for item in [any((0, 1))]}",
        "((NAME := lambda values: \"custom\", item)[1] for item in [any((0, 1))])",
    ] {
        for name in ["any", "all"] {
            let source = format!(
                "values = {}\nassert next(iter(values)) is True\n",
                expression.replace("NAME", name)
            );
            check(&source, 1).await;
            let mut mutation = tokio::process::Command::new(python());
            mutation.args(["-c", &source.replace("any((0, 1))", "all((0, 1))")]);
            let result = tokio::time::timeout(
                Duration::from_secs(20),
                mutation.kill_on_drop(true).output(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(!result.status.success());
            assert!(String::from_utf8_lossy(&result.stderr).contains("AssertionError"));
        }
    }
}

#[tokio::test]
async fn first_iterable_order_preserves_scope_and_flow_controls() {
    for (source, expected) in [
        ("g = ((all := 0) for item in [any((0, 1))])\n", 1),
        (
            "g = ((all := 0) for item in [any((0, 1))])\nresult = any((0, 1))\n",
            1,
        ),
        (
            "all = lambda values: True\nvalues = [(all := 0) for item in [any((0, 1))]]\n",
            0,
        ),
        (
            "values = ([(all := lambda values: True) for _ in range(1)], any((0, 1)))\n",
            0,
        ),
        (
            "values = [[(all := lambda values: True, item)[1] for _ in range(1)] for item in [any((0, 1))]]\nassert values == [[True]]\n",
            1,
        ),
        (
            "values = [item for item in [any((0, 1))] if (all := True)]\nassert values == [True]\n",
            1,
        ),
        (
            "def f():\n    return [(any := 0) for item in [any((0, 1))]]\ntry:\n    f()\nexcept UnboundLocalError:\n    pass\nelse:\n    raise AssertionError('expected static local')\n",
            0,
        ),
        (
            "def f():\n    return [(all := 0) for item in [any((0, 1))]]\nf()\n",
            0,
        ),
        (
            "for _ in range(2):\n    values = [(all := lambda values: True, item)[1] for item in [any((0, 1))]]\n",
            0,
        ),
        (
            "for _ in range(2):\n    values = ((all := lambda values: True, item)[1] for item in [any((0, 1))])\n    next(values)\n",
            0,
        ),
        ("assert (all := any((0, 1))) is True\n", 1),
        (
            "values = [(lambda: (all := 0))() for item in [any((0, 1))]]\n",
            1,
        ),
        (
            "items = [any((0, 1))]\nvalues = [(all := 0) for item in items]\n",
            1,
        ),
        (
            "def f(all):\n    return [(all := 0) for item in [any((0, 1))]]\nf(0)\n",
            0,
        ),
        (
            "def f():\n    global any\n    return [(any := 0) for item in [any((0, 1))]]\nf()\n",
            0,
        ),
        (
            "def outer():\n    from builtins import all\n    def inner():\n        nonlocal all\n        return [(all := 0) for item in [any((0, 1))]]\n    inner()\nouter()\n",
            0,
        ),
        (
            "values = [(lambda unused=(all := 0): item)() for item in [any((0, 1))]]\nassert values == [True]\n",
            1,
        ),
    ] {
        check(source, expected).await;
    }
}

#[tokio::test]
async fn first_iterable_order_applies_to_other_builtin_pairs() {
    for (original, replacement, operator) in [
        ("list", "tuple", "collection_list_tuple"),
        ("min", "max", "collection_min_max"),
        ("sorted", "reversed", "structure_sorted_reversed"),
    ] {
        for name in [original, replacement] {
            check_operator(
                &format!("values = [({name} := 0) for item in ({original}(range(2)),)]\n"),
                1,
                operator,
                replacement,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn first_iterable_mutation_is_detected_by_real_run() {
    let directory = tempfile::tempdir().unwrap();
    let source = "values = [(all := lambda values: 'custom', item)[1] for item in [any((0, 1))]]\nassert next(iter(values)) is True\n";
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
            "--jobs",
            "1",
            "--baseline-timeout",
            "5s",
            "--total-timeout",
            "10s",
            "--min-free-space",
            "1B",
            "--allow-best-effort-memory",
            "--",
        ])
        .arg(python())
        .arg("subject.py");
    let result = output(command).await;
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0, "{report}");
    assert_eq!(report["summary"]["counts"]["killed"], 1, "{report}");
    assert_eq!(report["mutants"].as_array().unwrap().len(), 1, "{report}");
    let candidate = &report["mutants"][0]["candidate"];
    assert_eq!(candidate["original"], "any");
    assert_eq!(candidate["replacement"], "all");
    assert_eq!(candidate["operator"], "collection_any_all");
    assert_eq!(
        candidate["span"]["start"],
        source.find("any((0, 1))").unwrap()
    );
    assert_eq!(candidate["span"]["length"], 3);
    assert_eq!(report["summary"]["complete"], true, "{report}");
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}
