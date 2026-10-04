use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_TEST_PYTHON").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
                "../../.venv/Scripts/python.exe"
            } else {
                "../../.venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

async fn invoke(source: &str, command: &str, check: &str) -> serde_json::Value {
    invoke_operator(source, command, check, "type_list_sequence").await
}

async fn invoke_operator(
    source: &str,
    command: &str,
    check: &str,
    operator: &str,
) -> serde_json::Value {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("subject.py"), source).unwrap();
    std::fs::write(directory.path().join("check.py"), check).unwrap();
    let mut args: Vec<OsString> = vec![
        "hoimin".into(),
        command.into(),
        "--root".into(),
        directory.path().as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        operator.into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
    ];
    if command == "run" {
        args.extend(["--format".into(), "json".into()]);
    }
    args.extend(["--".into(), python().into_os_string(), "check.py".into()]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(30),
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
    )
    .await
    .expect("public command timed out");
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
    serde_json::from_slice(&stdout).unwrap()
}

async fn plan(source: &str) -> PlanManifest {
    serde_json::from_value(invoke(source, "plan", "pass\n").await).unwrap()
}

fn evaluate(source: &str, expression: &str, expected: &str) {
    let output = Command::new(python()).args([
        "-c", "import sys, typing, collections.abc\nassert sys.version_info[:2] == (3, 14)\nns = {}\nexec(sys.argv[1], ns)\nactual = eval(sys.argv[2], ns)\nexpected = eval(sys.argv[3])\nassert actual == expected, (actual, expected)",
        source, expression, expected,
    ]).output().expect("CPython 3.14 required");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn deferred_import_sources_and_destinations_reject_later_rebinding() {
    for (import, name, binding) in [
        ("from typing import Sequence", "Sequence", "Sequence = set"),
        ("from typing import Sequence as Seq", "Seq", "Seq = set"),
        (
            "from collections.abc import Sequence as Seq",
            "Seq",
            "Seq = set",
        ),
        ("import typing", "typing.Sequence", "typing = object()"),
        ("import typing as t", "t.Sequence", "t = object()"),
        (
            "import collections.abc as abc",
            "abc.Sequence",
            "abc = object()",
        ),
    ] {
        for base in [name, "list"] {
            let source = format!("{import}\ndef f(x: {base}[int]): pass\n{binding}\n");
            assert!(plan(&source).await.candidates.is_empty(), "{source}");
            let source =
                format!("{import}\nclass C:\n    def f(x: {base}[int]): pass\n    {binding}\n");
            assert!(plan(&source).await.candidates.is_empty(), "{source}");
        }
    }
}

#[tokio::test]
async fn deferred_import_stable_positives_preserve_exact_pairs() {
    for (import, base, expected) in [
        (
            "from typing import Sequence",
            "Sequence",
            "typing.Sequence[int]",
        ),
        (
            "from collections.abc import Sequence as Seq",
            "Seq",
            "collections.abc.Sequence[int]",
        ),
        ("import typing", "typing.Sequence", "typing.Sequence[int]"),
        ("import typing as t", "t.Sequence", "typing.Sequence[int]"),
    ] {
        for (original, replacement, result) in [
            (format!("{base}[int]"), "list[int]".to_owned(), "list[int]"),
            ("list[int]".to_owned(), format!("{base}[int]"), expected),
        ] {
            let source = format!(
                "{import}\ndef f(x: {original}): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n"
            );
            let manifest = plan(&source).await;
            assert_eq!(manifest.candidates.len(), 1, "{source}");
            let candidate = &manifest.candidates[0];
            assert_eq!(candidate.original, original);
            assert_eq!(candidate.replacement, replacement);
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            assert_eq!(&source[start..end], original);
            let mutated = format!("{}{}{}", &source[..start], replacement, &source[end..]);
            evaluate(&mutated, "f.__annotations__['x']", result);
        }
    }
}

#[tokio::test]
async fn deferred_import_cache_and_restoration_precision_is_explicit() {
    for tail in [
        "cached = f.__annotations__\nSequence = set\n",
        "Sequence = set\nfrom typing import Sequence\n",
    ] {
        let source = format!("from typing import Sequence\ndef f(x: Sequence[int]): pass\n{tail}");
        evaluate(&source, "f.__annotations__['x']", "typing.Sequence[int]");
        assert!(
            plan(&source).await.candidates.is_empty(),
            "conservative exclusion: {source}"
        );
    }
    let source = "from typing import Sequence\nSequence = set\nfrom typing import Sequence\ndef f(x: Sequence[int]): pass\n";
    assert_eq!(plan(source).await.candidates.len(), 1);
}

#[tokio::test]
async fn deferred_import_alias_bounds_defaults_and_variables_reject_later_binding() {
    for declaration in [
        "type Alias = Sequence[int]",
        "def f[T: Sequence[int]](): pass",
        "class C[T = Sequence[int]]: pass",
        "x: Sequence[int]",
        "class C:\n    x: Sequence[int]",
        "def outer():\n    from typing import Sequence\n    def f(x: Sequence[int]): pass\n    Sequence = set",
    ] {
        let source = format!("from typing import Sequence\n{declaration}\nSequence = set\n");
        assert!(plan(&source).await.candidates.is_empty(), "{source}");
    }
}

#[tokio::test]
async fn deferred_import_public_run_does_not_count_invalid_pair_as_killed() {
    let source = "from typing import Sequence\ndef f(x: Sequence[int]): pass\nSequence = set\nobserved = f.__annotations__['x']\n";
    evaluate(source, "observed", "set[int]");
    let report = invoke(
        source,
        "run",
        "import subject\nassert subject.observed == set[int]\n",
    )
    .await;
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(report["summary"]["counts"]["killed"], 0);
    assert_eq!(report["summary"]["counts"]["survived"], 0);
}

#[tokio::test]
async fn deferred_import_late_exception_and_nonlocal_bindings_are_not_missed() {
    let mut failures = Vec::new();
    for source in [
        "from typing import Sequence\ndef f(x: Sequence[int]): pass\ntry:\n    raise ValueError()\nexcept ValueError as Sequence:\n    pass\n",
        "from typing import Sequence\nclass C:\n    Sequence: object\n    def f(x: Sequence[int]): pass\nSequence = set\n",
        "def outer():\n    from typing import Sequence\n    def middle():\n        def change():\n            nonlocal Sequence\n            Sequence = set\n        return change\n    def f(x: Sequence[int]): pass\n    middle()()\n    return f\n",
    ] {
        if !plan(source).await.candidates.is_empty() {
            failures.push(source);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

#[tokio::test]
async fn unevaluated_function_variable_annotations_keep_source_order_imports() {
    let source = "from typing import Sequence\ndef f():\n    global Sequence\n    Sequence = set\n    from typing import Sequence\n    value: list[int]\n";
    let manifest = plan(source).await;
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(manifest.candidates[0].replacement, "Sequence[int]");
}

#[tokio::test]
async fn deferred_import_local_owners_and_future_annotations_preserve_precision() {
    for (source, access, count, expected) in [
        (
            "from typing import Sequence\nclass C:\n    from typing import Sequence\n    def f(x: Sequence[int]): pass\nSequence = set\n",
            "C.f.__annotations__['x']",
            1,
            "typing.Sequence[int]",
        ),
        (
            "from typing import Sequence\ndef outer():\n    from typing import Sequence\n    def f(x: Sequence[int]): pass\n    return f\nf = outer()\nSequence = set\n",
            "f.__annotations__['x']",
            1,
            "typing.Sequence[int]",
        ),
        (
            "from __future__ import annotations\nfrom typing import Sequence\ndef f(x: Sequence[int]): pass\n",
            "f.__annotations__['x']",
            1,
            "'Sequence[int]'",
        ),
        (
            "from __future__ import annotations\nfrom typing import Sequence\ndef f(x: Sequence[int]): pass\nSequence = set\n",
            "f.__annotations__['x']",
            0,
            "'Sequence[int]'",
        ),
    ] {
        evaluate(source, access, expected);
        assert_eq!(plan(source).await.candidates.len(), count, "{source}");
    }
}

#[tokio::test]
async fn deferred_set_imports_share_provider_correct_stability_checks() {
    for (import, abstract_name, write) in [
        ("import collections.abc as abc", "abc.Set", "abc = object()"),
        (
            "from collections.abc import Set as Alias",
            "Alias",
            "Alias = list",
        ),
    ] {
        for base in ["set", abstract_name] {
            let source = format!("{import}\ndef f(x: {base}[int]): pass\n{write}\n");
            let manifest: PlanManifest = serde_json::from_value(
                invoke_operator(&source, "plan", "pass\n", "type_set_abstract_set").await,
            )
            .unwrap();
            assert!(manifest.candidates.is_empty(), "{source}");
        }
        let source = format!("{import}\ndef f(x: set[int]): pass\n");
        let manifest: PlanManifest = serde_json::from_value(
            invoke_operator(&source, "plan", "pass\n", "type_set_abstract_set").await,
        )
        .unwrap();
        assert_eq!(manifest.candidates.len(), 1);
        assert_eq!(
            manifest.candidates[0].replacement,
            format!("{abstract_name}[int]")
        );
        let mutated = source.replace("set[int]", &manifest.candidates[0].replacement);
        evaluate(
            &mutated,
            "f.__annotations__['x']",
            "collections.abc.Set[int]",
        );
    }
}

#[tokio::test]
async fn deferred_import_destination_uses_another_stable_alias() {
    let source =
        "from typing import Sequence as A, Sequence as B\ndef f(x: list[int]): pass\nA = set\n";
    let manifest = plan(source).await;
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(manifest.candidates[0].replacement, "B[int]");
    evaluate(
        &source.replace("list[int]", "B[int]"),
        "f.__annotations__['x']",
        "typing.Sequence[int]",
    );
}

#[tokio::test]
async fn deferred_unstable_imports_cannot_hide_prohibited_descendants() {
    let mut failures = Vec::new();
    for (source, expected) in [
        (
            "import typing as t\ndef f(x: list[t.Any]): pass\nt = type('Custom', (), {'Any': int})\n",
            "list[int]",
        ),
        (
            "import typing as t\ndef f(x: list[t.Literal[1]]): pass\nt = type('Custom', (), {'Literal': list})\n",
            "list[list[1]]",
        ),
        (
            "import typing as t\ndef f(x: dict[str, t.Any]): pass\nt = type('Custom', (), {'Any': int})\n",
            "dict[str, int]",
        ),
        (
            "from typing import Sequence as list\ndef f(x: dict[str, list[int]]): pass\nlist = set\n",
            "dict[str, set[int]]",
        ),
    ] {
        evaluate(source, "f.__annotations__['x']", expected);
        let manifest: PlanManifest = serde_json::from_value(
            invoke_operator(source, "plan", "pass\n", "type_nullable_add").await,
        )
        .unwrap();
        if !manifest.candidates.is_empty() {
            failures.push(source);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

#[tokio::test]
async fn conditional_import_provenance_loss_stays_ineligible() {
    for (binding, root, annotation, expected) in [
        (
            "import typing",
            "typing",
            "list[typing.Any]",
            "list[typing.Any]",
        ),
        ("import typing as q", "q", "list[q.Any]", "list[typing.Any]"),
        (
            "import typing as q",
            "q",
            "list[q.Literal[1]]",
            "list[typing.Literal[1]]",
        ),
    ] {
        let source = format!(
            "{binding}\ncondition = False\ncustom = type('Custom', (), {{'Any': int, 'Literal': list}})\nif condition:\n    {root} = custom\ndef f(x: {annotation}): pass\n"
        );
        evaluate(&source, "f.__annotations__['x']", expected);
        let manifest: PlanManifest = serde_json::from_value(
            invoke_operator(&source, "plan", "pass\n", "type_nullable_add").await,
        )
        .unwrap();
        assert!(manifest.candidates.is_empty(), "{source}");
    }
    for source in [
        "import typing as q\ndef f(x: q.Sequence[int]): pass\n",
        "def unrelated():\n    import typing as int\ndef f(x: int): pass\n",
        "def f():\n    import typing as q\n    x: q.Sequence[int]\n    q = object()\n",
    ] {
        let manifest: PlanManifest = serde_json::from_value(
            invoke_operator(source, "plan", "pass\n", "type_nullable_add").await,
        )
        .unwrap();
        assert_eq!(manifest.candidates.len(), 1, "{source}");
    }
}

#[tokio::test]
async fn local_conditional_import_provenance_loss_stays_ineligible() {
    let source = "def f():\n    import typing as q\n    if condition:\n        q = custom\n    x: list[q.Any]\n";
    let manifest: PlanManifest = serde_json::from_value(
        invoke_operator(source, "plan", "pass\n", "type_nullable_add").await,
    )
    .unwrap();
    assert_eq!(manifest.candidates, Vec::new());
}
