#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;

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

    args.extend(["--".into(), python().into_os_string(), "check.py".into()]);
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = tokio::time::timeout(
        Duration::from_secs(30),
        hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()),
    )
    .await
    .expect("public command timed out");
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    jsonl_report::from_slice(&stdout.snapshot()).unwrap()
}

async fn plan(source: &str) -> PlanManifest {
    serde_json::from_value(invoke(source, "plan", "pass\n").await).unwrap()
}

fn evaluate(source: &str, expression: &str, expected: &str) {
    let output = Command::new(python()).args([
        "-c", "import sys, typing, collections.abc\nassert sys.version_info[:2] in ((3, 14), (3, 15))\nns = {}\nexec(sys.argv[1], ns)\nactual = eval(sys.argv[2], ns)\nexpected = eval(sys.argv[3])\nassert actual == expected, (actual, expected)",
        source, expression, expected,
    ]).output().expect("CPython 3.14 or 3.15 required");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[rstest::rstest]
#[case::from_typing_import_sequence_1_provider_module(
    "from typing import Sequence\ndef f(x: Sequence[int]): pass\nSequence = set\n"
)]
#[case::from_typing_import_sequence_1_provider_class(
    "from typing import Sequence\nclass C:\n    def f(x: Sequence[int]): pass\n    Sequence = set\n"
)]
#[case::from_typing_import_sequence_1_builtin_module(
    "from typing import Sequence\ndef f(x: list[int]): pass\nSequence = set\n"
)]
#[case::from_typing_import_sequence_1_builtin_class(
    "from typing import Sequence\nclass C:\n    def f(x: list[int]): pass\n    Sequence = set\n"
)]
#[case::from_typing_import_sequence_as_seq_2_provider_module(
    "from typing import Sequence as Seq\ndef f(x: Seq[int]): pass\nSeq = set\n"
)]
#[case::from_typing_import_sequence_as_seq_2_provider_class(
    "from typing import Sequence as Seq\nclass C:\n    def f(x: Seq[int]): pass\n    Seq = set\n"
)]
#[case::from_typing_import_sequence_as_seq_2_builtin_module(
    "from typing import Sequence as Seq\ndef f(x: list[int]): pass\nSeq = set\n"
)]
#[case::from_typing_import_sequence_as_seq_2_builtin_class(
    "from typing import Sequence as Seq\nclass C:\n    def f(x: list[int]): pass\n    Seq = set\n"
)]
#[case::from_collections_abc_import_sequence_as_seq_3_provider_module(
    "from collections.abc import Sequence as Seq\ndef f(x: Seq[int]): pass\nSeq = set\n"
)]
#[case::from_collections_abc_import_sequence_as_seq_3_provider_class(
    "from collections.abc import Sequence as Seq\nclass C:\n    def f(x: Seq[int]): pass\n    Seq = set\n"
)]
#[case::from_collections_abc_import_sequence_as_seq_3_builtin_module(
    "from collections.abc import Sequence as Seq\ndef f(x: list[int]): pass\nSeq = set\n"
)]
#[case::from_collections_abc_import_sequence_as_seq_3_builtin_class(
    "from collections.abc import Sequence as Seq\nclass C:\n    def f(x: list[int]): pass\n    Seq = set\n"
)]
#[case::import_typing_4_provider_module(
    "import typing\ndef f(x: typing.Sequence[int]): pass\ntyping = object()\n"
)]
#[case::import_typing_4_provider_class(
    "import typing\nclass C:\n    def f(x: typing.Sequence[int]): pass\n    typing = object()\n"
)]
#[case::import_typing_4_builtin_module(
    "import typing\ndef f(x: list[int]): pass\ntyping = object()\n"
)]
#[case::import_typing_4_builtin_class(
    "import typing\nclass C:\n    def f(x: list[int]): pass\n    typing = object()\n"
)]
#[case::import_typing_as_t_5_provider_module(
    "import typing as t\ndef f(x: t.Sequence[int]): pass\nt = object()\n"
)]
#[case::import_typing_as_t_5_provider_class(
    "import typing as t\nclass C:\n    def f(x: t.Sequence[int]): pass\n    t = object()\n"
)]
#[case::import_typing_as_t_5_builtin_module(
    "import typing as t\ndef f(x: list[int]): pass\nt = object()\n"
)]
#[case::import_typing_as_t_5_builtin_class(
    "import typing as t\nclass C:\n    def f(x: list[int]): pass\n    t = object()\n"
)]
#[case::import_collections_abc_as_abc_6_provider_module(
    "import collections.abc as abc\ndef f(x: abc.Sequence[int]): pass\nabc = object()\n"
)]
#[case::import_collections_abc_as_abc_6_provider_class(
    "import collections.abc as abc\nclass C:\n    def f(x: abc.Sequence[int]): pass\n    abc = object()\n"
)]
#[case::import_collections_abc_as_abc_6_builtin_module(
    "import collections.abc as abc\ndef f(x: list[int]): pass\nabc = object()\n"
)]
#[case::import_collections_abc_as_abc_6_builtin_class(
    "import collections.abc as abc\nclass C:\n    def f(x: list[int]): pass\n    abc = object()\n"
)]
#[tokio::test]
async fn deferred_import_sources_and_destinations_reject_later_rebinding(#[case] source: &str) {
    let manifest = plan(source).await;

    assert!(manifest.candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::from_typing_import_sequence_1_from_provider(
    "from typing import Sequence\ndef f(x: Sequence[int]): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n",
    "Sequence[int]",
    "list[int]",
    "list[int]"
)]
#[case::from_typing_import_sequence_1_to_provider(
    "from typing import Sequence\ndef f(x: list[int]): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n",
    "list[int]",
    "Sequence[int]",
    "typing.Sequence[int]"
)]
#[case::from_collections_abc_import_sequence_as_seq_2_from_provider(
    "from collections.abc import Sequence as Seq\ndef f(x: Seq[int]): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n",
    "Seq[int]",
    "list[int]",
    "list[int]"
)]
#[case::from_collections_abc_import_sequence_as_seq_2_to_provider(
    "from collections.abc import Sequence as Seq\ndef f(x: list[int]): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n",
    "list[int]",
    "Seq[int]",
    "collections.abc.Sequence[int]"
)]
#[case::import_typing_3_from_provider(
    "import typing\ndef f(x: typing.Sequence[int]): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n",
    "typing.Sequence[int]",
    "list[int]",
    "list[int]"
)]
#[case::import_typing_3_to_provider(
    "import typing\ndef f(x: list[int]): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n",
    "list[int]",
    "typing.Sequence[int]",
    "typing.Sequence[int]"
)]
#[case::import_typing_as_t_4_from_provider(
    "import typing as t\ndef f(x: t.Sequence[int]): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n",
    "t.Sequence[int]",
    "list[int]",
    "list[int]"
)]
#[case::import_typing_as_t_4_to_provider(
    "import typing as t\ndef f(x: list[int]): pass\ndef unrelated():\n    Sequence = set\n    t = object()\n",
    "list[int]",
    "t.Sequence[int]",
    "typing.Sequence[int]"
)]
#[tokio::test]
async fn deferred_import_stable_positives_preserve_exact_pairs(
    #[case] source: &str,
    #[case] original: &str,
    #[case] replacement: &str,
    #[case] result: &str,
) {
    let manifest = plan(source).await;

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

#[rstest::rstest]
#[case::cached_f_annotations_sequence_set_1("cached = f.__annotations__\nSequence = set\n")]
#[case::sequence_set_from_typing_import_sequence_2("Sequence = set\nfrom typing import Sequence\n")]
#[tokio::test]
async fn deferred_import_cache_and_restoration_are_conservatively_excluded(#[case] tail: &str) {
    let source = format!("from typing import Sequence\ndef f(x: Sequence[int]): pass\n{tail}");
    evaluate(&source, "f.__annotations__['x']", "typing.Sequence[int]");
    assert!(
        plan(&source).await.candidates.is_empty(),
        "conservative exclusion: {source}"
    );
}

#[tokio::test]
async fn deferred_import_restored_before_definition_is_eligible() {
    let source = "from typing import Sequence\nSequence = set\nfrom typing import Sequence\ndef f(x: Sequence[int]): pass\n";
    assert_eq!(plan(source).await.candidates.len(), 1);
}

#[rstest::rstest]
#[case::type_alias_sequence_int_1("type Alias = Sequence[int]")]
#[case::def_f_t_sequence_int_pass_2("def f[T: Sequence[int]](): pass")]
#[case::class_c_t_sequence_int_pass_3("class C[T = Sequence[int]]: pass")]
#[case::x_sequence_int_4("x: Sequence[int]")]
#[case::class_c_x_sequence_int_5("class C:\n    x: Sequence[int]")]
#[case::def_outer_from_typing_import_sequence_def_f_x_sequence_6(
    "def outer():\n    from typing import Sequence\n    def f(x: Sequence[int]): pass\n    Sequence = set"
)]
#[tokio::test]
async fn deferred_import_alias_bounds_defaults_and_variables_reject_later_binding(
    #[case] declaration: &str,
) {
    let source = format!("from typing import Sequence\n{declaration}\nSequence = set\n");

    assert!(plan(&source).await.candidates.is_empty(), "{source}");
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

#[rstest::rstest]
#[case::from_typing_import_sequence_def_f_x_sequence_int_pass_t_1(
    "from typing import Sequence\ndef f(x: Sequence[int]): pass\ntry:\n    raise ValueError()\nexcept ValueError as Sequence:\n    pass\n"
)]
#[case::from_typing_import_sequence_class_c_sequence_object_def_2(
    "from typing import Sequence\nclass C:\n    Sequence: object\n    def f(x: Sequence[int]): pass\nSequence = set\n"
)]
#[case::def_outer_from_typing_import_sequence_def_middle_def_ch_3(
    "def outer():\n    from typing import Sequence\n    def middle():\n        def change():\n            nonlocal Sequence\n            Sequence = set\n        return change\n    def f(x: Sequence[int]): pass\n    middle()()\n    return f\n"
)]
#[tokio::test]
async fn deferred_import_late_exception_and_nonlocal_bindings_are_not_missed(#[case] source: &str) {
    assert!(plan(source).await.candidates.is_empty(), "{source}");
}

#[tokio::test]
async fn unevaluated_function_variable_annotations_keep_source_order_imports() {
    let source = "from typing import Sequence\ndef f():\n    global Sequence\n    Sequence = set\n    from typing import Sequence\n    value: list[int]\n";
    let manifest = plan(source).await;
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(manifest.candidates[0].replacement, "Sequence[int]");
}

#[rstest::rstest]
#[case::from_typing_import_sequence_class_c_from_typing_import_1(
    "from typing import Sequence\nclass C:\n    from typing import Sequence\n    def f(x: Sequence[int]): pass\nSequence = set\n",
    "C.f.__annotations__['x']",
    1,
    "typing.Sequence[int]"
)]
#[case::from_typing_import_sequence_def_outer_from_typing_impor_2(
    "from typing import Sequence\ndef outer():\n    from typing import Sequence\n    def f(x: Sequence[int]): pass\n    return f\nf = outer()\nSequence = set\n",
    "f.__annotations__['x']",
    1,
    "typing.Sequence[int]"
)]
#[case::from_future_import_annotations_from_typing_import_seque_3(
    "from __future__ import annotations\nfrom typing import Sequence\ndef f(x: Sequence[int]): pass\n",
    "f.__annotations__['x']",
    1,
    "'Sequence[int]'"
)]
#[case::from_future_import_annotations_from_typing_import_seque_4(
    "from __future__ import annotations\nfrom typing import Sequence\ndef f(x: Sequence[int]): pass\nSequence = set\n",
    "f.__annotations__['x']",
    0,
    "'Sequence[int]'"
)]
#[tokio::test]
async fn deferred_import_local_owners_and_future_annotations_preserve_precision(
    #[case] source: &str,
    #[case] access: &str,
    #[case] count: usize,
    #[case] expected: &str,
) {
    evaluate(source, access, expected);

    assert_eq!(plan(source).await.candidates.len(), count, "{source}");
}

#[rstest::rstest]
#[case::import_collections_abc_as_abc_1_builtin(
    "import collections.abc as abc\ndef f(x: set[int]): pass\nabc = object()\n"
)]
#[case::import_collections_abc_as_abc_1_provider(
    "import collections.abc as abc\ndef f(x: abc.Set[int]): pass\nabc = object()\n"
)]
#[case::from_collections_abc_import_set_as_alias_2_builtin(
    "from collections.abc import Set as Alias\ndef f(x: set[int]): pass\nAlias = list\n"
)]
#[case::from_collections_abc_import_set_as_alias_2_provider(
    "from collections.abc import Set as Alias\ndef f(x: Alias[int]): pass\nAlias = list\n"
)]
#[tokio::test]
async fn deferred_set_imports_reject_unstable_providers(#[case] source: &str) {
    let manifest: PlanManifest = serde_json::from_value(
        invoke_operator(source, "plan", "pass\n", "type_set_abstract_set").await,
    )
    .unwrap();

    assert!(manifest.candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::import_collections_abc_as_abc_1(
    "import collections.abc as abc",
    "abc.Set",
    "abc = object()"
)]
#[case::from_collections_abc_import_set_as_alias_2(
    "from collections.abc import Set as Alias",
    "Alias",
    "Alias = list"
)]
#[tokio::test]
async fn deferred_set_imports_keep_stable_providers(
    #[case] import: &str,
    #[case] abstract_name: &str,
    #[case] write: &str,
) {
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

    let _ = write;
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

#[rstest::rstest]
#[case::import_typing_as_t_def_f_x_list_t_any_pass_t_type_custo_1(
    "import typing as t\ndef f(x: list[t.Any]): pass\nt = type('Custom', (), {'Any': int})\n",
    "list[int]"
)]
#[case::import_typing_as_t_def_f_x_list_t_literal_1_pass_t_type_2(
    "import typing as t\ndef f(x: list[t.Literal[1]]): pass\nt = type('Custom', (), {'Literal': list})\n",
    "list[list[1]]"
)]
#[case::import_typing_as_t_def_f_x_dict_str_t_any_pass_t_type_c_3(
    "import typing as t\ndef f(x: dict[str, t.Any]): pass\nt = type('Custom', (), {'Any': int})\n",
    "dict[str, int]"
)]
#[case::from_typing_import_sequence_as_list_def_f_x_dict_str_li_4(
    "from typing import Sequence as list\ndef f(x: dict[str, list[int]]): pass\nlist = set\n",
    "dict[str, set[int]]"
)]
#[tokio::test]
async fn deferred_unstable_imports_cannot_hide_prohibited_descendants(
    #[case] source: &str,
    #[case] expected: &str,
) {
    evaluate(source, "f.__annotations__['x']", expected);

    let manifest: PlanManifest = serde_json::from_value(
        invoke_operator(source, "plan", "pass\n", "type_nullable_add").await,
    )
    .unwrap();

    assert!(manifest.candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::import_typing_1("import typing", "typing", "list[typing.Any]", "list[typing.Any]")]
#[case::import_typing_as_q_2("import typing as q", "q", "list[q.Any]", "list[typing.Any]")]
#[case::import_typing_as_q_3(
    "import typing as q",
    "q",
    "list[q.Literal[1]]",
    "list[typing.Literal[1]]"
)]
#[tokio::test]
async fn conditional_import_provenance_loss_stays_ineligible_1(
    #[case] binding: &str,
    #[case] root: &str,
    #[case] annotation: &str,
    #[case] expected: &str,
) {
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

#[rstest::rstest]
#[case::import_typing_as_q_def_f_x_q_sequence_int_pass_1(
    "import typing as q\ndef f(x: q.Sequence[int]): pass\n"
)]
#[case::def_unrelated_import_typing_as_int_def_f_x_int_pass_2(
    "def unrelated():\n    import typing as int\ndef f(x: int): pass\n"
)]
#[case::def_f_import_typing_as_q_x_q_sequence_int_q_object_3(
    "def f():\n    import typing as q\n    x: q.Sequence[int]\n    q = object()\n"
)]
#[tokio::test]
async fn conditional_import_provenance_loss_stays_ineligible_2(#[case] source: &str) {
    let manifest: PlanManifest = serde_json::from_value(
        invoke_operator(source, "plan", "pass\n", "type_nullable_add").await,
    )
    .unwrap();

    assert_eq!(manifest.candidates.len(), 1, "{source}");
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
