#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
#[path = "support/mutation_outcome.rs"]
mod mutation_outcome;
use collecting_writer::CollectingWriter;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use hoimin_cli::plan::PlanManifest;

fn python() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

async fn invoke(source: &str, mode: &str, operator: &str) -> serde_json::Value {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("subject.py"), source).unwrap();
    let mut args: Vec<OsString> = vec![
        "hoimin".into(),
        mode.into(),
        "--root".into(),
        project.path().into(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        operator.into(),
        "--jobs".into(),
        "1".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--allow-best-effort-memory".into(),
    ];

    args.extend([
        "--".into(),
        python().into(),
        "-c".into(),
        "import subject".into(),
    ]);
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let exit = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;
    // The import-only command leaves the valid mutant alive (score below 1).
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    jsonl_report::from_slice(&stdout.snapshot()).unwrap()
}

fn evaluate(source: &str, expected: &str) {
    let output = Command::new(python()).args([
        "-c", "import sys, typing, collections.abc; ns = {}; exec(sys.argv[1], ns); assert ns['f'].__annotations__['x'] == eval(sys.argv[2]), ns['f'].__annotations__", source, expected,
    ]).output().unwrap();
    assert!(
        output.status.success(),
        "{source}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn provider_set_pairs_generate_evaluable_annotations() {
    for (imports, abstract_name, runtime_type) in [
        (
            "import collections.abc",
            "collections.abc.Set",
            "collections.abc.Set[int]",
        ),
        (
            "import collections.abc as abc",
            "abc.Set",
            "collections.abc.Set[int]",
        ),
        (
            "from collections.abc import Set",
            "Set",
            "collections.abc.Set[int]",
        ),
        (
            "from collections.abc import Set as ASet",
            "ASet",
            "collections.abc.Set[int]",
        ),
        (
            "import typing",
            "typing.AbstractSet",
            "typing.AbstractSet[int]",
        ),
        (
            "import typing as t",
            "t.AbstractSet",
            "typing.AbstractSet[int]",
        ),
        (
            "from typing import AbstractSet",
            "AbstractSet",
            "typing.AbstractSet[int]",
        ),
        (
            "from typing import AbstractSet as ASet",
            "ASet",
            "typing.AbstractSet[int]",
        ),
        (
            "import typing as t\nimport collections.abc as abc",
            "abc.Set",
            "collections.abc.Set[int]",
        ),
        (
            "from typing import AbstractSet as ZSet\nfrom collections.abc import Set as ASet",
            "ASet",
            "collections.abc.Set[int]",
        ),
    ] {
        for (original, replacement, original_type, replacement_type) in [
            (
                "set[int]".to_owned(),
                format!("{abstract_name}[int]"),
                "set[int]",
                runtime_type,
            ),
            (
                format!("{abstract_name}[int]"),
                "set[int]".to_owned(),
                runtime_type,
                "set[int]",
            ),
        ] {
            let source = format!("{imports}\ndef f(x: {original}): pass\n");
            let plan: PlanManifest =
                serde_json::from_value(invoke(&source, "plan", "type_set_abstract_set").await)
                    .unwrap();
            assert_eq!(plan.candidates.len(), 1, "{source}");
            let candidate = &plan.candidates[0];
            assert_eq!(candidate.original, original);
            assert_eq!(candidate.replacement, replacement);
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            assert_eq!(&source[start..end], original);
            let mutated = format!("{}{}{}", &source[..start], replacement, &source[end..]);
            evaluate(&source, original_type);
            evaluate(&mutated, replacement_type);
        }
    }
}

#[tokio::test]
async fn provider_set_guards_and_nullable_recognition() {
    for source in [
        "from typing import Set\ndef f(x: set[int]): pass\n",
        "from typing import Set\ndef f(x: Set[int]): pass\n",
        "import collections.abc as abc\ndef f(x: abc.AbstractSet[int]): pass\n",
        "from collections.abc import Set\nSet = list\ndef f(x: Set[int]): pass\n",
        "import collections.abc as abc\nabc = object()\ndef f(x: set[int]): pass\n",
        "from collections.abc import Set\nset = list\ndef f(x: Set[int]): pass\n",
    ] {
        let plan: PlanManifest =
            serde_json::from_value(invoke(source, "plan", "type_set_abstract_set").await).unwrap();
        assert!(plan.candidates.is_empty(), "{source}");
    }
    let source = "import collections.abc as abc\ndef f(x: abc.Set[int]): pass\n";
    let plan: PlanManifest =
        serde_json::from_value(invoke(source, "plan", "type_nullable_add").await).unwrap();
    assert_eq!(plan.candidates.len(), 1);
    assert_eq!(plan.candidates[0].replacement, "abc.Set[int] | None");
}

#[tokio::test]
async fn provider_set_public_run_survives_annotation_evaluation() {
    let report = invoke("import collections.abc as abc\ndef f(x: set[int]): pass\nobserved = f.__annotations__['x']\n", "run", "type_set_abstract_set").await;
    assert_eq!(report["mutants"].as_array().unwrap().len(), 1);
    assert_eq!(mutation_outcome::status(&report["mutants"][0]), "survived");
}
