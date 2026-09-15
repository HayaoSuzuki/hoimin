use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/collection-annotation.jsonl");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u8,
    id: String,
    source: String,
    access: String,
    operator: String,
    original: String,
    replacement: String,
    present: bool,
    evaluation_error: bool,
}

fn cases() -> Vec<Case> {
    CORPUS
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

fn evaluate(source: &str, case: &Case, mutated: bool) {
    evaluate_expression(
        source,
        &format!("({}).__annotations__['value']", case.access),
        &case.id,
        case.evaluation_error && !mutated,
        if mutated { &case.replacement } else { "" },
    );
}

fn evaluate_expression(
    source: &str,
    access: &str,
    id: &str,
    evaluation_error: bool,
    replacement: &str,
) {
    let harness = r"
import sys, typing, collections.abc, builtins
assert sys.version_info[:2] == (3, 14), sys.version
source, access, evaluation_error, replacement = sys.argv[1:]
namespace = {}
code = compile(source, '<annotation-oracle>', 'exec')
exec(code, namespace)
try:
    annotation = eval(access, namespace)
except TypeError:
    assert evaluation_error == 'true'
else:
    assert evaluation_error == 'false'
    if replacement:
        expected = eval(replacement, dict(vars(typing), list=builtins.list, set=builtins.set, dict=builtins.dict))
        assert annotation == expected, (annotation, expected)
";
    let output = Command::new(python())
        .args([
            "-c",
            harness,
            source,
            access,
            if evaluation_error { "true" } else { "false" },
            replacement,
        ])
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("infrastructure error: CPython3.14 must be installed in .venv");
    assert!(
        output.status.success(),
        "{}: {}",
        id,
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn plan(source: &str, operator: &str) -> PlanManifest {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("subject.py"), source).unwrap();
    let args = [
        OsString::from("hoimin"),
        "plan".into(),
        "--root".into(),
        directory.path().as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        operator.into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--".into(),
        python().into_os_string(),
        "-c".into(),
        "pass".into(),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
    )
    .await
    .expect("infrastructure error: public plan timed out");
    assert_eq!(
        code,
        0,
        "infrastructure error: {}",
        String::from_utf8_lossy(&stderr)
    );
    serde_json::from_slice(&stdout).unwrap()
}

#[test]
fn corpus_schema_and_original_annotation_evaluation() {
    let cases = cases();
    assert_eq!(cases.len(), 114);
    let mut ids = std::collections::BTreeSet::new();
    for case in cases {
        assert_eq!(case.schema, 1);
        assert!(ids.insert(case.id.clone()));
        assert_eq!(case.source.matches(&case.original).count(), 1);
        evaluate(&case.source, &case, false);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn collection_annotation_public_plan_matches_lean() {
    let mut mismatches = Vec::new();
    for case in cases() {
        let manifest = plan(&case.source, &case.operator).await;
        if manifest.candidates.len() != usize::from(case.present) {
            mismatches.push(format!(
                "{}: expected {}, observed {}",
                case.id,
                usize::from(case.present),
                manifest.candidates.len()
            ));
            continue;
        }
        for candidate in &manifest.candidates {
            assert_eq!(candidate.original, case.original, "{}", case.id);
            assert_eq!(candidate.replacement, case.replacement, "{}", case.id);
            assert_eq!(candidate.operator, case.operator, "{}", case.id);
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            assert_eq!(&case.source[start..end], case.original);
            let mutated = format!(
                "{}{}{}",
                &case.source[..start],
                candidate.replacement,
                &case.source[end..]
            );
            evaluate(&mutated, &case, true);
        }
    }
    assert!(
        mismatches.is_empty(),
        "same-premise mismatches:\n{}",
        mismatches.join("\n")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn shadowed_list_does_not_suppress_abstract_pair() {
    let source = "from typing import Sequence, Iterable\nlist = tuple\ndef record(value: Sequence[int]):\n    pass\n";
    let manifest = plan(source, "type_sequence_iterable").await;
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(manifest.candidates[0].replacement, "Iterable[int]");
}

#[tokio::test(flavor = "current_thread")]
async fn collection_annotation_non_header_sites_and_aliases() {
    for (concrete, abstract_name, args, operator) in [
        ("list", "Sequence", "int", "type_list_sequence"),
        ("set", "AbstractSet", "int", "type_set_abstract_set"),
        ("dict", "Mapping", "int, str", "type_dict_mapping"),
    ] {
        for base in [concrete, abstract_name] {
            let annotation = format!("{base}[{args}]");
            for (body, count) in [
                (format!("value: {annotation}\n{concrete} = tuple\n"), 0),
                (
                    format!("class Box:\n    value: {annotation}\n    {concrete} = tuple\n"),
                    0,
                ),
                (
                    format!("def record():\n    value: {annotation}\n    {concrete} = tuple\n"),
                    0,
                ),
                (
                    format!("def record[{concrete}]() -> {annotation}:\n    pass\n"),
                    0,
                ),
                (
                    format!("def record({concrete}: {annotation}):\n    pass\n"),
                    1,
                ),
                (
                    format!("def record() -> {annotation}:\n    {concrete} = tuple\n"),
                    1,
                ),
            ] {
                let source = format!("from typing import {abstract_name}\n{body}");
                let manifest = plan(&source, operator).await;
                assert_eq!(manifest.candidates.len(), count, "{source}");
            }
        }
        for import in [
            format!("from typing import {abstract_name} as Alias"),
            "import typing as t".to_owned(),
        ] {
            let base = if import.contains(" as Alias") {
                "Alias".to_owned()
            } else {
                format!("t.{abstract_name}")
            };
            let source = format!("{import}\ndef record(value: {base}[{args}]):\n    pass\n");
            let manifest = plan(&source, operator).await;
            assert_eq!(manifest.candidates.len(), 1, "{source}");
            assert_eq!(
                manifest.candidates[0].replacement,
                format!("{concrete}[{args}]")
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn collection_annotation_alias_values_bounds_and_defaults() {
    for (concrete, abstract_name, args, operator) in [
        ("list", "Sequence", "int", "type_list_sequence"),
        ("set", "AbstractSet", "int", "type_set_abstract_set"),
        ("dict", "Mapping", "int, str", "type_dict_mapping"),
    ] {
        for base in [concrete, abstract_name] {
            let annotation = format!("{base}[{args}]");
            for (body, count, access) in [
                (format!("type Alias = {annotation}\n"), 1, "Alias.__value__"),
                (
                    format!("type Alias[{concrete}] = {annotation}\n"),
                    0,
                    "Alias.__value__",
                ),
                (
                    format!("def record[T: {annotation}]():\n    pass\n"),
                    1,
                    "record.__type_params__[-1].__bound__",
                ),
                (
                    format!("def record[{concrete}, T: {annotation}]():\n    pass\n"),
                    0,
                    "record.__type_params__[-1].__bound__",
                ),
                (
                    format!("class Box[T = {annotation}]:\n    pass\n"),
                    1,
                    "Box.__type_params__[-1].__default__",
                ),
                (
                    format!("class Box[{concrete}, T = {annotation}]:\n    pass\n"),
                    0,
                    "Box.__type_params__[-1].__default__",
                ),
                (
                    format!("type Alias[T = {annotation}] = int\n"),
                    1,
                    "Alias.__type_params__[-1].__default__",
                ),
                (
                    format!("type Alias[{concrete}, T = {annotation}] = int\n"),
                    0,
                    "Alias.__type_params__[-1].__default__",
                ),
            ] {
                let source = format!("from typing import {abstract_name}\n{body}");
                evaluate_expression(&source, access, &body, count == 0 && base == concrete, "");
                let manifest = plan(&source, operator).await;
                assert_eq!(manifest.candidates.len(), count, "{source}");
                for candidate in manifest.candidates {
                    let start = usize::try_from(candidate.span.start).unwrap();
                    let end = start + usize::try_from(candidate.span.length).unwrap();
                    assert_eq!(&source[start..end], annotation);
                    let mutated = format!(
                        "{}{}{}",
                        &source[..start],
                        candidate.replacement,
                        &source[end..]
                    );
                    evaluate_expression(&mutated, access, &body, false, &candidate.replacement);
                }
            }
        }
    }
}
