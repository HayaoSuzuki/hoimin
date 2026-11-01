#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;

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
assert sys.version_info[:2] in ((3, 14), (3, 15)), sys.version
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
        .expect("infrastructure error: CPython 3.14 or 3.15 must be installed in .venv");
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
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()),
    )
    .await
    .expect("infrastructure error: public plan timed out");
    assert_eq!(
        code,
        0,
        "infrastructure error: {}",
        String::from_utf8_lossy(&stderr.snapshot())
    );
    serde_json::from_slice(&stdout.snapshot()).unwrap()
}

#[test]
fn corpus_schema_and_original_annotation_evaluation() {
    let cases = cases();
    assert_eq!(cases.len(), 121);
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
    for case in cases() {
        let manifest = plan(&case.source, &case.operator).await;
        assert_eq!(
            manifest.candidates.len(),
            usize::from(case.present),
            "{}",
            case.id
        );
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
}

#[tokio::test(flavor = "current_thread")]
async fn shadowed_list_does_not_suppress_abstract_pair() {
    let source = "from typing import Sequence, Iterable\nlist = tuple\ndef record(value: Sequence[int]):\n    pass\n";
    let manifest = plan(source, "type_sequence_iterable").await;
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(manifest.candidates[0].replacement, "Iterable[int]");
}
#[rstest::rstest]
#[case::list_1_list_site_1(
    "from typing import Sequence\nvalue: list[int]\nlist = tuple\n",
    "type_list_sequence",
    0
)]
#[case::list_1_list_site_2(
    "from typing import Sequence\nclass Box:\n    value: list[int]\n    list = tuple\n",
    "type_list_sequence",
    0
)]
#[case::list_1_list_site_3(
    "from typing import Sequence\ndef record():\n    value: list[int]\n    list = tuple\n",
    "type_list_sequence",
    0
)]
#[case::list_1_list_site_4(
    "from typing import Sequence\ndef record[list]() -> list[int]:\n    pass\n",
    "type_list_sequence",
    0
)]
#[case::list_1_list_site_5(
    "from typing import Sequence\ndef record(list: list[int]):\n    pass\n",
    "type_list_sequence",
    1
)]
#[case::list_1_list_site_6(
    "from typing import Sequence\ndef record() -> list[int]:\n    list = tuple\n",
    "type_list_sequence",
    1
)]
#[case::list_1_sequence_site_1(
    "from typing import Sequence\nvalue: Sequence[int]\nlist = tuple\n",
    "type_list_sequence",
    0
)]
#[case::list_1_sequence_site_2(
    "from typing import Sequence\nclass Box:\n    value: Sequence[int]\n    list = tuple\n",
    "type_list_sequence",
    0
)]
#[case::list_1_sequence_site_3(
    "from typing import Sequence\ndef record():\n    value: Sequence[int]\n    list = tuple\n",
    "type_list_sequence",
    0
)]
#[case::list_1_sequence_site_4(
    "from typing import Sequence\ndef record[list]() -> Sequence[int]:\n    pass\n",
    "type_list_sequence",
    0
)]
#[case::list_1_sequence_site_5(
    "from typing import Sequence\ndef record(list: Sequence[int]):\n    pass\n",
    "type_list_sequence",
    1
)]
#[case::list_1_sequence_site_6(
    "from typing import Sequence\ndef record() -> Sequence[int]:\n    list = tuple\n",
    "type_list_sequence",
    1
)]
#[case::set_2_set_site_1(
    "from typing import AbstractSet\nvalue: set[int]\nset = tuple\n",
    "type_set_abstract_set",
    0
)]
#[case::set_2_set_site_2(
    "from typing import AbstractSet\nclass Box:\n    value: set[int]\n    set = tuple\n",
    "type_set_abstract_set",
    0
)]
#[case::set_2_set_site_3(
    "from typing import AbstractSet\ndef record():\n    value: set[int]\n    set = tuple\n",
    "type_set_abstract_set",
    0
)]
#[case::set_2_set_site_4(
    "from typing import AbstractSet\ndef record[set]() -> set[int]:\n    pass\n",
    "type_set_abstract_set",
    0
)]
#[case::set_2_set_site_5(
    "from typing import AbstractSet\ndef record(set: set[int]):\n    pass\n",
    "type_set_abstract_set",
    1
)]
#[case::set_2_set_site_6(
    "from typing import AbstractSet\ndef record() -> set[int]:\n    set = tuple\n",
    "type_set_abstract_set",
    1
)]
#[case::set_2_abstractset_site_1(
    "from typing import AbstractSet\nvalue: AbstractSet[int]\nset = tuple\n",
    "type_set_abstract_set",
    0
)]
#[case::set_2_abstractset_site_2(
    "from typing import AbstractSet\nclass Box:\n    value: AbstractSet[int]\n    set = tuple\n",
    "type_set_abstract_set",
    0
)]
#[case::set_2_abstractset_site_3(
    "from typing import AbstractSet\ndef record():\n    value: AbstractSet[int]\n    set = tuple\n",
    "type_set_abstract_set",
    0
)]
#[case::set_2_abstractset_site_4(
    "from typing import AbstractSet\ndef record[set]() -> AbstractSet[int]:\n    pass\n",
    "type_set_abstract_set",
    0
)]
#[case::set_2_abstractset_site_5(
    "from typing import AbstractSet\ndef record(set: AbstractSet[int]):\n    pass\n",
    "type_set_abstract_set",
    1
)]
#[case::set_2_abstractset_site_6(
    "from typing import AbstractSet\ndef record() -> AbstractSet[int]:\n    set = tuple\n",
    "type_set_abstract_set",
    1
)]
#[case::dict_3_dict_site_1(
    "from typing import Mapping\nvalue: dict[int, str]\ndict = tuple\n",
    "type_dict_mapping",
    0
)]
#[case::dict_3_dict_site_2(
    "from typing import Mapping\nclass Box:\n    value: dict[int, str]\n    dict = tuple\n",
    "type_dict_mapping",
    0
)]
#[case::dict_3_dict_site_3(
    "from typing import Mapping\ndef record():\n    value: dict[int, str]\n    dict = tuple\n",
    "type_dict_mapping",
    0
)]
#[case::dict_3_dict_site_4(
    "from typing import Mapping\ndef record[dict]() -> dict[int, str]:\n    pass\n",
    "type_dict_mapping",
    0
)]
#[case::dict_3_dict_site_5(
    "from typing import Mapping\ndef record(dict: dict[int, str]):\n    pass\n",
    "type_dict_mapping",
    1
)]
#[case::dict_3_dict_site_6(
    "from typing import Mapping\ndef record() -> dict[int, str]:\n    dict = tuple\n",
    "type_dict_mapping",
    1
)]
#[case::dict_3_mapping_site_1(
    "from typing import Mapping\nvalue: Mapping[int, str]\ndict = tuple\n",
    "type_dict_mapping",
    0
)]
#[case::dict_3_mapping_site_2(
    "from typing import Mapping\nclass Box:\n    value: Mapping[int, str]\n    dict = tuple\n",
    "type_dict_mapping",
    0
)]
#[case::dict_3_mapping_site_3(
    "from typing import Mapping\ndef record():\n    value: Mapping[int, str]\n    dict = tuple\n",
    "type_dict_mapping",
    0
)]
#[case::dict_3_mapping_site_4(
    "from typing import Mapping\ndef record[dict]() -> Mapping[int, str]:\n    pass\n",
    "type_dict_mapping",
    0
)]
#[case::dict_3_mapping_site_5(
    "from typing import Mapping\ndef record(dict: Mapping[int, str]):\n    pass\n",
    "type_dict_mapping",
    1
)]
#[case::dict_3_mapping_site_6(
    "from typing import Mapping\ndef record() -> Mapping[int, str]:\n    dict = tuple\n",
    "type_dict_mapping",
    1
)]
#[tokio::test(flavor = "current_thread")]
async fn collection_annotation_non_header_sites(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] count: usize,
) {
    let manifest = plan(source, operator).await;

    assert_eq!(manifest.candidates.len(), count, "{source}");
}

#[rstest::rstest]
#[case::list_1_alias(
    "from typing import Sequence as Alias\ndef record(value: Alias[int]):\n    pass\n",
    "type_list_sequence",
    "list[int]"
)]
#[case::list_1_t_sequence(
    "import typing as t\ndef record(value: t.Sequence[int]):\n    pass\n",
    "type_list_sequence",
    "list[int]"
)]
#[case::set_2_alias(
    "from typing import AbstractSet as Alias\ndef record(value: Alias[int]):\n    pass\n",
    "type_set_abstract_set",
    "set[int]"
)]
#[case::set_2_t_abstractset(
    "import typing as t\ndef record(value: t.AbstractSet[int]):\n    pass\n",
    "type_set_abstract_set",
    "set[int]"
)]
#[case::dict_3_alias(
    "from typing import Mapping as Alias\ndef record(value: Alias[int, str]):\n    pass\n",
    "type_dict_mapping",
    "dict[int, str]"
)]
#[case::dict_3_t_mapping(
    "import typing as t\ndef record(value: t.Mapping[int, str]):\n    pass\n",
    "type_dict_mapping",
    "dict[int, str]"
)]
#[tokio::test(flavor = "current_thread")]
async fn collection_annotation_aliases_preserve_destination_spelling(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] expected: &str,
) {
    let manifest = plan(source, operator).await;

    assert_eq!(manifest.candidates.len(), 1, "{source}");
    assert_eq!(manifest.candidates[0].replacement, expected);
}

#[rstest::rstest]
#[case::list_1_list_site_1(
    "from typing import Sequence\ntype Alias = list[int]\n",
    "type_list_sequence",
    1,
    "list[int]",
    "type Alias = list[int]\n",
    "Alias.__value__",
    false
)]
#[case::list_1_list_site_2(
    "from typing import Sequence\ntype Alias[list] = list[int]\n",
    "type_list_sequence",
    0,
    "list[int]",
    "type Alias[list] = list[int]\n",
    "Alias.__value__",
    true
)]
#[case::list_1_list_site_3(
    "from typing import Sequence\ndef record[T: list[int]]():\n    pass\n",
    "type_list_sequence",
    1,
    "list[int]",
    "def record[T: list[int]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::list_1_list_site_4(
    "from typing import Sequence\ndef record[list, T: list[int]]():\n    pass\n",
    "type_list_sequence",
    0,
    "list[int]",
    "def record[list, T: list[int]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    true
)]
#[case::list_1_list_site_5(
    "from typing import Sequence\nclass Box[T = list[int]]:\n    pass\n",
    "type_list_sequence",
    1,
    "list[int]",
    "class Box[T = list[int]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::list_1_list_site_6(
    "from typing import Sequence\nclass Box[list, T = list[int]]:\n    pass\n",
    "type_list_sequence",
    0,
    "list[int]",
    "class Box[list, T = list[int]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    true
)]
#[case::list_1_list_site_7(
    "from typing import Sequence\ntype Alias[T = list[int]] = int\n",
    "type_list_sequence",
    1,
    "list[int]",
    "type Alias[T = list[int]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[case::list_1_list_site_8(
    "from typing import Sequence\ntype Alias[list, T = list[int]] = int\n",
    "type_list_sequence",
    0,
    "list[int]",
    "type Alias[list, T = list[int]] = int\n",
    "Alias.__type_params__[-1].__default__",
    true
)]
#[case::list_1_sequence_site_1(
    "from typing import Sequence\ntype Alias = Sequence[int]\n",
    "type_list_sequence",
    1,
    "Sequence[int]",
    "type Alias = Sequence[int]\n",
    "Alias.__value__",
    false
)]
#[case::list_1_sequence_site_2(
    "from typing import Sequence\ntype Alias[list] = Sequence[int]\n",
    "type_list_sequence",
    0,
    "Sequence[int]",
    "type Alias[list] = Sequence[int]\n",
    "Alias.__value__",
    false
)]
#[case::list_1_sequence_site_3(
    "from typing import Sequence\ndef record[T: Sequence[int]]():\n    pass\n",
    "type_list_sequence",
    1,
    "Sequence[int]",
    "def record[T: Sequence[int]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::list_1_sequence_site_4(
    "from typing import Sequence\ndef record[list, T: Sequence[int]]():\n    pass\n",
    "type_list_sequence",
    0,
    "Sequence[int]",
    "def record[list, T: Sequence[int]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::list_1_sequence_site_5(
    "from typing import Sequence\nclass Box[T = Sequence[int]]:\n    pass\n",
    "type_list_sequence",
    1,
    "Sequence[int]",
    "class Box[T = Sequence[int]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::list_1_sequence_site_6(
    "from typing import Sequence\nclass Box[list, T = Sequence[int]]:\n    pass\n",
    "type_list_sequence",
    0,
    "Sequence[int]",
    "class Box[list, T = Sequence[int]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::list_1_sequence_site_7(
    "from typing import Sequence\ntype Alias[T = Sequence[int]] = int\n",
    "type_list_sequence",
    1,
    "Sequence[int]",
    "type Alias[T = Sequence[int]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[case::list_1_sequence_site_8(
    "from typing import Sequence\ntype Alias[list, T = Sequence[int]] = int\n",
    "type_list_sequence",
    0,
    "Sequence[int]",
    "type Alias[list, T = Sequence[int]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[case::set_2_set_site_1(
    "from typing import AbstractSet\ntype Alias = set[int]\n",
    "type_set_abstract_set",
    1,
    "set[int]",
    "type Alias = set[int]\n",
    "Alias.__value__",
    false
)]
#[case::set_2_set_site_2(
    "from typing import AbstractSet\ntype Alias[set] = set[int]\n",
    "type_set_abstract_set",
    0,
    "set[int]",
    "type Alias[set] = set[int]\n",
    "Alias.__value__",
    true
)]
#[case::set_2_set_site_3(
    "from typing import AbstractSet\ndef record[T: set[int]]():\n    pass\n",
    "type_set_abstract_set",
    1,
    "set[int]",
    "def record[T: set[int]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::set_2_set_site_4(
    "from typing import AbstractSet\ndef record[set, T: set[int]]():\n    pass\n",
    "type_set_abstract_set",
    0,
    "set[int]",
    "def record[set, T: set[int]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    true
)]
#[case::set_2_set_site_5(
    "from typing import AbstractSet\nclass Box[T = set[int]]:\n    pass\n",
    "type_set_abstract_set",
    1,
    "set[int]",
    "class Box[T = set[int]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::set_2_set_site_6(
    "from typing import AbstractSet\nclass Box[set, T = set[int]]:\n    pass\n",
    "type_set_abstract_set",
    0,
    "set[int]",
    "class Box[set, T = set[int]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    true
)]
#[case::set_2_set_site_7(
    "from typing import AbstractSet\ntype Alias[T = set[int]] = int\n",
    "type_set_abstract_set",
    1,
    "set[int]",
    "type Alias[T = set[int]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[case::set_2_set_site_8(
    "from typing import AbstractSet\ntype Alias[set, T = set[int]] = int\n",
    "type_set_abstract_set",
    0,
    "set[int]",
    "type Alias[set, T = set[int]] = int\n",
    "Alias.__type_params__[-1].__default__",
    true
)]
#[case::set_2_abstractset_site_1(
    "from typing import AbstractSet\ntype Alias = AbstractSet[int]\n",
    "type_set_abstract_set",
    1,
    "AbstractSet[int]",
    "type Alias = AbstractSet[int]\n",
    "Alias.__value__",
    false
)]
#[case::set_2_abstractset_site_2(
    "from typing import AbstractSet\ntype Alias[set] = AbstractSet[int]\n",
    "type_set_abstract_set",
    0,
    "AbstractSet[int]",
    "type Alias[set] = AbstractSet[int]\n",
    "Alias.__value__",
    false
)]
#[case::set_2_abstractset_site_3(
    "from typing import AbstractSet\ndef record[T: AbstractSet[int]]():\n    pass\n",
    "type_set_abstract_set",
    1,
    "AbstractSet[int]",
    "def record[T: AbstractSet[int]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::set_2_abstractset_site_4(
    "from typing import AbstractSet\ndef record[set, T: AbstractSet[int]]():\n    pass\n",
    "type_set_abstract_set",
    0,
    "AbstractSet[int]",
    "def record[set, T: AbstractSet[int]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::set_2_abstractset_site_5(
    "from typing import AbstractSet\nclass Box[T = AbstractSet[int]]:\n    pass\n",
    "type_set_abstract_set",
    1,
    "AbstractSet[int]",
    "class Box[T = AbstractSet[int]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::set_2_abstractset_site_6(
    "from typing import AbstractSet\nclass Box[set, T = AbstractSet[int]]:\n    pass\n",
    "type_set_abstract_set",
    0,
    "AbstractSet[int]",
    "class Box[set, T = AbstractSet[int]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::set_2_abstractset_site_7(
    "from typing import AbstractSet\ntype Alias[T = AbstractSet[int]] = int\n",
    "type_set_abstract_set",
    1,
    "AbstractSet[int]",
    "type Alias[T = AbstractSet[int]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[case::set_2_abstractset_site_8(
    "from typing import AbstractSet\ntype Alias[set, T = AbstractSet[int]] = int\n",
    "type_set_abstract_set",
    0,
    "AbstractSet[int]",
    "type Alias[set, T = AbstractSet[int]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[case::dict_3_dict_site_1(
    "from typing import Mapping\ntype Alias = dict[int, str]\n",
    "type_dict_mapping",
    1,
    "dict[int, str]",
    "type Alias = dict[int, str]\n",
    "Alias.__value__",
    false
)]
#[case::dict_3_dict_site_2(
    "from typing import Mapping\ntype Alias[dict] = dict[int, str]\n",
    "type_dict_mapping",
    0,
    "dict[int, str]",
    "type Alias[dict] = dict[int, str]\n",
    "Alias.__value__",
    true
)]
#[case::dict_3_dict_site_3(
    "from typing import Mapping\ndef record[T: dict[int, str]]():\n    pass\n",
    "type_dict_mapping",
    1,
    "dict[int, str]",
    "def record[T: dict[int, str]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::dict_3_dict_site_4(
    "from typing import Mapping\ndef record[dict, T: dict[int, str]]():\n    pass\n",
    "type_dict_mapping",
    0,
    "dict[int, str]",
    "def record[dict, T: dict[int, str]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    true
)]
#[case::dict_3_dict_site_5(
    "from typing import Mapping\nclass Box[T = dict[int, str]]:\n    pass\n",
    "type_dict_mapping",
    1,
    "dict[int, str]",
    "class Box[T = dict[int, str]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::dict_3_dict_site_6(
    "from typing import Mapping\nclass Box[dict, T = dict[int, str]]:\n    pass\n",
    "type_dict_mapping",
    0,
    "dict[int, str]",
    "class Box[dict, T = dict[int, str]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    true
)]
#[case::dict_3_dict_site_7(
    "from typing import Mapping\ntype Alias[T = dict[int, str]] = int\n",
    "type_dict_mapping",
    1,
    "dict[int, str]",
    "type Alias[T = dict[int, str]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[case::dict_3_dict_site_8(
    "from typing import Mapping\ntype Alias[dict, T = dict[int, str]] = int\n",
    "type_dict_mapping",
    0,
    "dict[int, str]",
    "type Alias[dict, T = dict[int, str]] = int\n",
    "Alias.__type_params__[-1].__default__",
    true
)]
#[case::dict_3_mapping_site_1(
    "from typing import Mapping\ntype Alias = Mapping[int, str]\n",
    "type_dict_mapping",
    1,
    "Mapping[int, str]",
    "type Alias = Mapping[int, str]\n",
    "Alias.__value__",
    false
)]
#[case::dict_3_mapping_site_2(
    "from typing import Mapping\ntype Alias[dict] = Mapping[int, str]\n",
    "type_dict_mapping",
    0,
    "Mapping[int, str]",
    "type Alias[dict] = Mapping[int, str]\n",
    "Alias.__value__",
    false
)]
#[case::dict_3_mapping_site_3(
    "from typing import Mapping\ndef record[T: Mapping[int, str]]():\n    pass\n",
    "type_dict_mapping",
    1,
    "Mapping[int, str]",
    "def record[T: Mapping[int, str]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::dict_3_mapping_site_4(
    "from typing import Mapping\ndef record[dict, T: Mapping[int, str]]():\n    pass\n",
    "type_dict_mapping",
    0,
    "Mapping[int, str]",
    "def record[dict, T: Mapping[int, str]]():\n    pass\n",
    "record.__type_params__[-1].__bound__",
    false
)]
#[case::dict_3_mapping_site_5(
    "from typing import Mapping\nclass Box[T = Mapping[int, str]]:\n    pass\n",
    "type_dict_mapping",
    1,
    "Mapping[int, str]",
    "class Box[T = Mapping[int, str]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::dict_3_mapping_site_6(
    "from typing import Mapping\nclass Box[dict, T = Mapping[int, str]]:\n    pass\n",
    "type_dict_mapping",
    0,
    "Mapping[int, str]",
    "class Box[dict, T = Mapping[int, str]]:\n    pass\n",
    "Box.__type_params__[-1].__default__",
    false
)]
#[case::dict_3_mapping_site_7(
    "from typing import Mapping\ntype Alias[T = Mapping[int, str]] = int\n",
    "type_dict_mapping",
    1,
    "Mapping[int, str]",
    "type Alias[T = Mapping[int, str]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[case::dict_3_mapping_site_8(
    "from typing import Mapping\ntype Alias[dict, T = Mapping[int, str]] = int\n",
    "type_dict_mapping",
    0,
    "Mapping[int, str]",
    "type Alias[dict, T = Mapping[int, str]] = int\n",
    "Alias.__type_params__[-1].__default__",
    false
)]
#[tokio::test(flavor = "current_thread")]
async fn collection_annotation_alias_values_bounds_and_defaults(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] count: usize,
    #[case] annotation: &str,
    #[case] body: &str,
    #[case] access: &str,
    #[case] evaluation_error: bool,
) {
    evaluate_expression(source, access, body, evaluation_error, "");

    let manifest = plan(source, operator).await;

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
        evaluate_expression(&mutated, access, body, false, &candidate.replacement);
    }
}
