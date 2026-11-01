#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn python() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    })
}

fn observe(root: &Path, harness: &str) -> String {
    let output = Command::new(python())
        .args(["-c", harness])
        .current_dir(root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n")
}

async fn assert_plan(source: &str, operator: &str, original: &str, replacement: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("subject.py");
    fs::write(&path, source).unwrap();
    observe(
        directory.path(),
        "compile(open('subject.py').read(), 'subject.py', 'exec')",
    );
    let args = [
        OsString::from("hoimin"),
        OsString::from("plan"),
        OsString::from("--root"),
        directory.path().as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("subject.py"),
        OsString::from("--operators"),
        OsString::from(operator),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--min-free-space"),
        OsString::from("1B"),
        OsString::from("--"),
        python().into_os_string(),
        OsString::from("-c"),
        OsString::from("pass"),
    ];
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    assert_eq!(
        hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await,
        0,
        "{}",
        String::from_utf8_lossy(&stderr.snapshot())
    );
    let manifest: serde_json::Value = serde_json::from_slice(&stdout.snapshot()).unwrap();
    let candidates: Vec<_> = manifest["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["original"] == original && c["replacement"] == replacement)
        .collect();
    let expected: BTreeSet<_> = source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("# keep"))
        .map(|(index, _)| u64::try_from(index + 1).unwrap())
        .collect();
    let mut offset = 0;
    let expected_starts: BTreeSet<_> = source
        .split_inclusive('\n')
        .filter_map(|line| {
            let start = line
                .contains("# keep")
                .then(|| offset + line.find(original).unwrap());
            offset += line.len();
            start
        })
        .collect();
    let actual_starts: BTreeSet<_> = candidates
        .iter()
        .map(|c| usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap())
        .collect();
    assert_eq!(actual_starts, expected_starts, "{source}\n{manifest}");
    let actual: BTreeSet<_> = candidates
        .iter()
        .map(|c| c["line"].as_u64().unwrap())
        .collect();
    assert_eq!(actual, expected, "{source}\n{manifest}");
    assert_eq!(candidates.len(), expected.len(), "{manifest}");
    let mut ids = BTreeSet::new();
    for candidate in candidates {
        assert_eq!(candidate["original"], original);
        assert_eq!(candidate["replacement"], replacement);
        let id = candidate["id"].as_str().unwrap();
        let identity = hoimin_core::CandidateIdentity {
            schema_version: hoimin_core::CANDIDATE_SCHEMA_VERSION,
            file_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            path: "subject.py".into(),
            span: serde_json::from_value(candidate["span"].clone()).unwrap(),
            operator: operator.to_owned(),
            replacement: replacement.to_owned(),
        };
        assert_eq!(id, hoimin_core::stable_mutant_id(&identity).as_str());
        assert!(ids.insert(id));
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(&source[start..start + length], original);
        assert_eq!(
            source[..start].bytes().filter(|b| *b == b'\n').count() + 1,
            usize::try_from(candidate["line"].as_u64().unwrap()).unwrap()
        );
    }
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[rstest::rstest]
#[case::collection_list_tuple_1_original_plain_function("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("list", "", "def Generic", "()"))]
#[case::collection_list_tuple_1_original_plain_class("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("list", "", "class Generic", ""))]
#[case::collection_list_tuple_1_original_tuple_function("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("list", "*", "def Generic", "()"))]
#[case::collection_list_tuple_1_original_tuple_class("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("list", "*", "class Generic", ""))]
#[case::collection_list_tuple_1_original_paramspec_function("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("list", "**", "def Generic", "()"))]
#[case::collection_list_tuple_1_original_paramspec_class("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("list", "**", "class Generic", ""))]
#[case::collection_list_tuple_1_replacement_plain_function("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("tuple", "", "def Generic", "()"))]
#[case::collection_list_tuple_1_replacement_plain_class("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("tuple", "", "class Generic", ""))]
#[case::collection_list_tuple_1_replacement_tuple_function("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("tuple", "*", "def Generic", "()"))]
#[case::collection_list_tuple_1_replacement_tuple_class("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("tuple", "*", "class Generic", ""))]
#[case::collection_list_tuple_1_replacement_paramspec_function("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("tuple", "**", "def Generic", "()"))]
#[case::collection_list_tuple_1_replacement_paramspec_class("collection_list_tuple", "list", "tuple", "value = list((1, 2))", ("tuple", "**", "class Generic", ""))]
#[case::collection_any_all_2_original_plain_function("collection_any_all", "any", "all", "value = any((1, 2))", ("any", "", "def Generic", "()"))]
#[case::collection_any_all_2_original_plain_class("collection_any_all", "any", "all", "value = any((1, 2))", ("any", "", "class Generic", ""))]
#[case::collection_any_all_2_original_tuple_function("collection_any_all", "any", "all", "value = any((1, 2))", ("any", "*", "def Generic", "()"))]
#[case::collection_any_all_2_original_tuple_class("collection_any_all", "any", "all", "value = any((1, 2))", ("any", "*", "class Generic", ""))]
#[case::collection_any_all_2_original_paramspec_function("collection_any_all", "any", "all", "value = any((1, 2))", ("any", "**", "def Generic", "()"))]
#[case::collection_any_all_2_original_paramspec_class("collection_any_all", "any", "all", "value = any((1, 2))", ("any", "**", "class Generic", ""))]
#[case::collection_any_all_2_replacement_plain_function("collection_any_all", "any", "all", "value = any((1, 2))", ("all", "", "def Generic", "()"))]
#[case::collection_any_all_2_replacement_plain_class("collection_any_all", "any", "all", "value = any((1, 2))", ("all", "", "class Generic", ""))]
#[case::collection_any_all_2_replacement_tuple_function("collection_any_all", "any", "all", "value = any((1, 2))", ("all", "*", "def Generic", "()"))]
#[case::collection_any_all_2_replacement_tuple_class("collection_any_all", "any", "all", "value = any((1, 2))", ("all", "*", "class Generic", ""))]
#[case::collection_any_all_2_replacement_paramspec_function("collection_any_all", "any", "all", "value = any((1, 2))", ("all", "**", "def Generic", "()"))]
#[case::collection_any_all_2_replacement_paramspec_class("collection_any_all", "any", "all", "value = any((1, 2))", ("all", "**", "class Generic", ""))]
#[case::collection_min_max_3_original_plain_function("collection_min_max", "min", "max", "value = min((1, 2))", ("min", "", "def Generic", "()"))]
#[case::collection_min_max_3_original_plain_class("collection_min_max", "min", "max", "value = min((1, 2))", ("min", "", "class Generic", ""))]
#[case::collection_min_max_3_original_tuple_function("collection_min_max", "min", "max", "value = min((1, 2))", ("min", "*", "def Generic", "()"))]
#[case::collection_min_max_3_original_tuple_class("collection_min_max", "min", "max", "value = min((1, 2))", ("min", "*", "class Generic", ""))]
#[case::collection_min_max_3_original_paramspec_function("collection_min_max", "min", "max", "value = min((1, 2))", ("min", "**", "def Generic", "()"))]
#[case::collection_min_max_3_original_paramspec_class("collection_min_max", "min", "max", "value = min((1, 2))", ("min", "**", "class Generic", ""))]
#[case::collection_min_max_3_replacement_plain_function("collection_min_max", "min", "max", "value = min((1, 2))", ("max", "", "def Generic", "()"))]
#[case::collection_min_max_3_replacement_plain_class("collection_min_max", "min", "max", "value = min((1, 2))", ("max", "", "class Generic", ""))]
#[case::collection_min_max_3_replacement_tuple_function("collection_min_max", "min", "max", "value = min((1, 2))", ("max", "*", "def Generic", "()"))]
#[case::collection_min_max_3_replacement_tuple_class("collection_min_max", "min", "max", "value = min((1, 2))", ("max", "*", "class Generic", ""))]
#[case::collection_min_max_3_replacement_paramspec_function("collection_min_max", "min", "max", "value = min((1, 2))", ("max", "**", "def Generic", "()"))]
#[case::collection_min_max_3_replacement_paramspec_class("collection_min_max", "min", "max", "value = min((1, 2))", ("max", "**", "class Generic", ""))]
#[case::structure_sorted_reversed_4_original_plain_function("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("sorted", "", "def Generic", "()"))]
#[case::structure_sorted_reversed_4_original_plain_class("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("sorted", "", "class Generic", ""))]
#[case::structure_sorted_reversed_4_original_tuple_function("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("sorted", "*", "def Generic", "()"))]
#[case::structure_sorted_reversed_4_original_tuple_class("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("sorted", "*", "class Generic", ""))]
#[case::structure_sorted_reversed_4_original_paramspec_function("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("sorted", "**", "def Generic", "()"))]
#[case::structure_sorted_reversed_4_original_paramspec_class("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("sorted", "**", "class Generic", ""))]
#[case::structure_sorted_reversed_4_replacement_plain_function("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("reversed", "", "def Generic", "()"))]
#[case::structure_sorted_reversed_4_replacement_plain_class("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("reversed", "", "class Generic", ""))]
#[case::structure_sorted_reversed_4_replacement_tuple_function("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("reversed", "*", "def Generic", "()"))]
#[case::structure_sorted_reversed_4_replacement_tuple_class("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("reversed", "*", "class Generic", ""))]
#[case::structure_sorted_reversed_4_replacement_paramspec_function("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("reversed", "**", "def Generic", "()"))]
#[case::structure_sorted_reversed_4_replacement_paramspec_class("structure_sorted_reversed", "sorted", "reversed", "value = sorted((1, 2))", ("reversed", "**", "class Generic", ""))]
#[case::exception_type_pair_5_original_plain_function("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("ValueError", "", "def Generic", "()"))]
#[case::exception_type_pair_5_original_plain_class("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("ValueError", "", "class Generic", ""))]
#[case::exception_type_pair_5_original_tuple_function("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("ValueError", "*", "def Generic", "()"))]
#[case::exception_type_pair_5_original_tuple_class("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("ValueError", "*", "class Generic", ""))]
#[case::exception_type_pair_5_original_paramspec_function("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("ValueError", "**", "def Generic", "()"))]
#[case::exception_type_pair_5_original_paramspec_class("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("ValueError", "**", "class Generic", ""))]
#[case::exception_type_pair_5_replacement_plain_function("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("TypeError", "", "def Generic", "()"))]
#[case::exception_type_pair_5_replacement_plain_class("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("TypeError", "", "class Generic", ""))]
#[case::exception_type_pair_5_replacement_tuple_function("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("TypeError", "*", "def Generic", "()"))]
#[case::exception_type_pair_5_replacement_tuple_class("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("TypeError", "*", "class Generic", ""))]
#[case::exception_type_pair_5_replacement_paramspec_function("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("TypeError", "**", "def Generic", "()"))]
#[case::exception_type_pair_5_replacement_paramspec_class("exception_type_pair", "ValueError", "TypeError", "raise ValueError()", ("TypeError", "**", "class Generic", ""))]
#[tokio::test]
async fn generic_builtin_pairs_respect_both_endpoints_and_restore_outer_scope(
    #[case] operator: &str,
    #[case] original: &str,
    #[case] replacement: &str,
    #[case] statement: &str,
    #[case] header: (&str, &str, &str, &str),
) {
    let (name, prefix, declaration, suffix) = header;

    let source = format!(
        "{statement} # keep\n{declaration}[{prefix}{name}]{suffix}:\n    {statement}\n    def nested():\n        {statement}\n    values = [{original}((1, 2)) for item in (1,)]\n{statement} # keep\ndef outside():\n    {statement} # keep\n"
    );
    assert_plan(&source, operator, original, replacement).await;
}

#[tokio::test]
async fn generic_headers_and_bodies_follow_distinct_class_lookup_paths() {
    let source = r"def decorate(value):
    return lambda obj: obj
@decorate(list(())) # keep
def f[tuple](arg=list(()), *, kw=list(())): # keep
    value = list(())
    positive = any(())
class Outer[tuple]:
    all = object()
    class Inner[T](list(())):
        value = list(())
        other = any(())
        def method(self):
            return any(())
@decorate(list(())) # keep
class C[tuple](list(()), marker=list(())):
    value = list(())
value = list(()) # keep
";
    // Each marked line must contain exactly one selected candidate.
    let source = source.replace("arg=list(()), *, kw=list(())", "arg=list(())");
    assert_plan(&source, "collection_list_tuple", "list", "tuple").await;
    assert_plan(
        r"class Outer:
    all = object()
    class Inner[T](any(())):
        value = any(()) # prepared namespace
        def method(self):
            return any(()) # keep
    def generic[T](self, arg=any(())):
        return any(()) # keep
    class Comp[T](marker=[any(()) for _ in (0,)]): # keep
        pass
    class Iter[T](marker=[item for item in any(())]):
        pass
value = any(()) # keep
",
        "collection_any_all",
        "any",
        "all",
    )
    .await;
}

#[rstest::rstest]
#[case::class_early_t_list_items_keep_value_list_items_prepared_1(
    "class Early[T](list(items)): # keep\n    value = list(items) # prepared namespace\ntuple = object()\nclass Late[T](list(items)):\n    value = list(items)\n"
)]
#[case::for_item_in_items_class_c_t_list_items_value_list_items_2(
    "for item in items:\n    class C[T](list(items)):\n        value = list(items)\n    tuple = object()\n"
)]
#[case::try_work_except_exception_as_list_class_c_t_list_items_3(
    "try:\n    work()\nexcept Exception as list:\n    class C[T](list(items)):\n        value = list(items)\nvalue = list(items) # keep\n"
)]
#[case::class_outer_try_work_except_exception_as_list_class_c_t_4(
    "class Outer:\n    try:\n        work()\n    except Exception as list:\n        class C[T](list(items)):\n            value = list(items) # prepared namespace\n"
)]
#[tokio::test]
async fn generic_headers_preserve_ordered_and_temporary_outer_bindings(#[case] source: &str) {
    assert_plan(source, "collection_list_tuple", "list", "tuple").await;
}

#[rstest::rstest]
#[case::def_generic_tuple_global_tuple_return_list_items_keep_c_1(
    "def generic[tuple]():\n    global tuple\n    return list(items) # keep\nclass C[tuple]:\n    global tuple\n    value = list(items) # keep\n"
)]
#[case::def_outer_tuple_object_def_generic_t_nonlocal_tuple_ret_2(
    "def outer():\n    tuple = object()\n    def generic[T]():\n        nonlocal tuple\n        return list(items)\n    return list(items)\nvalue = list(items) # keep\n"
)]
#[case::def_generic_tuple_def_inner_global_tuple_return_list_it_3(
    "def generic[tuple]():\n    def inner():\n        global tuple\n        return list(items) # keep\n    return list(items)\nvalue = list(items) # keep\n"
)]
#[case::def_generic_t_value_list_items_tuple_object_class_c_t_v_4(
    "def generic[T]():\n    value = list(items)\n    tuple = object()\nclass C[T]:\n    value = list(items) # keep\n    tuple = object()\n    later = list(items)\n    def method(self):\n        return list(items) # keep\n"
)]
#[tokio::test]
async fn generic_directives_and_local_overrides_preserve_lexical_rules(#[case] source: &str) {
    assert_plan(source, "collection_list_tuple", "list", "tuple").await;
}

#[test]
fn cpython_observes_generic_header_body_and_lazy_annotation_bindings() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        observe(
            directory.path(),
            include_str!("fixtures/type_parameter_bindings.py")
        ),
        "header/body/lazy identities OK\n"
    );
}

#[tokio::test]
async fn generic_type_positions_stay_excluded_while_runtime_controls_remain() {
    assert_plan(
        r"def f[tuple: list(items) = list(items)](
    arg: list(items) =
    list(items), # keep
) -> list(items):
    def nested(arg=list(items)):
        return list(items)
    return list(items)
class C[tuple]:
    annotation: list(items)
    def method(self, arg=list(items)):
        return list(items)
type Alias[tuple] = list(items)
value = list(items) # keep
",
        "collection_list_tuple",
        "list",
        "tuple",
    )
    .await;
    assert_plan(
        r"def f[tuple](arg=any(items)): # keep
    value = any(items) # keep
    def nested():
        return any(items) # keep
    values = [any(items) for item in items] # keep
class C[tuple](any(items), marker=42): # keep
    value = any(items) # prepared namespace
    def method(self):
        return any(items) # keep
value = any(items) # keep
",
        "collection_any_all",
        "any",
        "all",
    )
    .await;
    assert_plan("def f[tuple](arg=42, *, kw=list(items)): # keep\n    return list(items)\nclass C[T](marker=list(items)): # keep\n    value = list(items) # prepared namespace\n", "collection_list_tuple", "list", "tuple").await;
}

#[rstest::rstest]
#[case::def_method_self_return_list_range_2_1(
    "    def method(self):\n        return list(range(2))\n"
)]
#[case::def_method_t_self_return_list_range_2_2(
    "    def method[T](self):\n        return list(range(2))\n"
)]
#[case::method_lambda_self_list_range_2_3("    method = lambda self: list(range(2))\n")]
#[case::values_list_range_2_for_in_range_1_4("    values = [list(range(2)) for _ in range(1)]\n")]
#[case::values_list_range_2_for_in_range_1_5("    values = (list(range(2)) for _ in range(1))\n")]
#[tokio::test]
async fn class_global_directives_do_not_override_descendant_type_parameter_capture(
    #[case] body: &str,
) {
    let source = format!(
        "class C[tuple]:\n    global tuple\n{body}    direct = list(range(2)) # keep\n    def explicit_global(self):\n        global tuple\n        return list(range(2)) # keep\nvalue = list(range(2)) # keep\n"
    );
    assert_plan(&source, "collection_list_tuple", "list", "tuple").await;
}

#[tokio::test]
async fn class_global_directives_do_not_override_ordinary_enclosing_function_bindings() {
    assert_plan(
        "def outer():\n    tuple = object()\n    class C:\n        global tuple\n        def method(self):\n            return list(range(2))\n        direct = list(range(2)) # keep\n    return C\nvalue = list(range(2)) # keep\n",
        "collection_list_tuple",
        "list",
        "tuple",
    )
    .await;
}
