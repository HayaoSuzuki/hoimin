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

#[rstest::rstest]
#[case::import_collections_abc_1(
    "import collections.abc",
    "collections.abc.Set",
    "collections.abc.Set[int]"
)]
#[case::import_collections_abc_as_abc_2(
    "import collections.abc as abc",
    "abc.Set",
    "collections.abc.Set[int]"
)]
#[case::from_collections_abc_import_set_3(
    "from collections.abc import Set",
    "Set",
    "collections.abc.Set[int]"
)]
#[case::from_collections_abc_import_set_as_aset_4(
    "from collections.abc import Set as ASet",
    "ASet",
    "collections.abc.Set[int]"
)]
#[case::import_typing_5("import typing", "typing.AbstractSet", "typing.AbstractSet[int]")]
#[case::import_typing_as_t_6("import typing as t", "t.AbstractSet", "typing.AbstractSet[int]")]
#[case::from_typing_import_abstractset_7(
    "from typing import AbstractSet",
    "AbstractSet",
    "typing.AbstractSet[int]"
)]
#[case::from_typing_import_abstractset_as_aset_8(
    "from typing import AbstractSet as ASet",
    "ASet",
    "typing.AbstractSet[int]"
)]
#[case::import_typing_as_t_import_collections_abc_as_abc_9(
    "import typing as t\nimport collections.abc as abc",
    "abc.Set",
    "collections.abc.Set[int]"
)]
#[case::from_typing_import_abstractset_as_zset_from_collections_10(
    "from typing import AbstractSet as ZSet\nfrom collections.abc import Set as ASet",
    "ASet",
    "collections.abc.Set[int]"
)]
#[tokio::test]
async fn provider_set_pairs_generate_evaluable_annotations_from_builtin(
    #[case] imports: &str,
    #[case] abstract_name: &str,
    #[case] runtime_type: &str,
) {
    let (original, replacement, original_type, replacement_type) = (
        "set[int]".to_owned(),
        format!("{abstract_name}[int]"),
        "set[int]",
        runtime_type,
    );

    let source = format!("{imports}\ndef f(x: {original}): pass\n");

    let plan: PlanManifest =
        serde_json::from_value(invoke(&source, "plan", "type_set_abstract_set").await).unwrap();

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

#[rstest::rstest]
#[case::import_collections_abc_1(
    "import collections.abc",
    "collections.abc.Set",
    "collections.abc.Set[int]"
)]
#[case::import_collections_abc_as_abc_2(
    "import collections.abc as abc",
    "abc.Set",
    "collections.abc.Set[int]"
)]
#[case::from_collections_abc_import_set_3(
    "from collections.abc import Set",
    "Set",
    "collections.abc.Set[int]"
)]
#[case::from_collections_abc_import_set_as_aset_4(
    "from collections.abc import Set as ASet",
    "ASet",
    "collections.abc.Set[int]"
)]
#[case::import_typing_5("import typing", "typing.AbstractSet", "typing.AbstractSet[int]")]
#[case::import_typing_as_t_6("import typing as t", "t.AbstractSet", "typing.AbstractSet[int]")]
#[case::from_typing_import_abstractset_7(
    "from typing import AbstractSet",
    "AbstractSet",
    "typing.AbstractSet[int]"
)]
#[case::from_typing_import_abstractset_as_aset_8(
    "from typing import AbstractSet as ASet",
    "ASet",
    "typing.AbstractSet[int]"
)]
#[case::import_typing_as_t_import_collections_abc_as_abc_9(
    "import typing as t\nimport collections.abc as abc",
    "abc.Set",
    "collections.abc.Set[int]"
)]
#[case::from_typing_import_abstractset_as_zset_from_collections_10(
    "from typing import AbstractSet as ZSet\nfrom collections.abc import Set as ASet",
    "ASet",
    "collections.abc.Set[int]"
)]
#[tokio::test]
async fn provider_set_pairs_generate_evaluable_annotations_to_builtin(
    #[case] imports: &str,
    #[case] abstract_name: &str,
    #[case] runtime_type: &str,
) {
    let (original, replacement, original_type, replacement_type) = (
        format!("{abstract_name}[int]"),
        "set[int]".to_owned(),
        runtime_type,
        "set[int]",
    );

    let source = format!("{imports}\ndef f(x: {original}): pass\n");

    let plan: PlanManifest =
        serde_json::from_value(invoke(&source, "plan", "type_set_abstract_set").await).unwrap();

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

#[rstest::rstest]
#[case::from_typing_import_set_def_f_x_set_int_pass_1(
    "from typing import Set\ndef f(x: set[int]): pass\n"
)]
#[case::from_typing_import_set_def_f_x_set_int_pass_2(
    "from typing import Set\ndef f(x: Set[int]): pass\n"
)]
#[case::import_collections_abc_as_abc_def_f_x_abc_abstractset_i_3(
    "import collections.abc as abc\ndef f(x: abc.AbstractSet[int]): pass\n"
)]
#[case::from_collections_abc_import_set_set_list_def_f_x_set_in_4(
    "from collections.abc import Set\nSet = list\ndef f(x: Set[int]): pass\n"
)]
#[case::import_collections_abc_as_abc_abc_object_def_f_x_set_in_5(
    "import collections.abc as abc\nabc = object()\ndef f(x: set[int]): pass\n"
)]
#[case::from_collections_abc_import_set_set_list_def_f_x_set_in_6(
    "from collections.abc import Set\nset = list\ndef f(x: Set[int]): pass\n"
)]
#[tokio::test]
async fn provider_set_guards_reject_unproven_providers(#[case] source: &str) {
    let plan: PlanManifest =
        serde_json::from_value(invoke(source, "plan", "type_set_abstract_set").await).unwrap();
    assert!(plan.candidates.is_empty(), "{source}");
}

#[tokio::test]
async fn provider_set_is_recognized_as_nullable() {
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
