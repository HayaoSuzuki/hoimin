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
    String::from_utf8(output.stdout).unwrap()
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
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await,
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let manifest: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
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

#[tokio::test]
async fn generic_builtin_pairs_respect_both_endpoints_and_restore_outer_scope() {
    for (operator, original, replacement) in [
        ("collection_list_tuple", "list", "tuple"),
        ("collection_any_all", "any", "all"),
        ("collection_min_max", "min", "max"),
        ("structure_sorted_reversed", "sorted", "reversed"),
        ("exception_type_pair", "ValueError", "TypeError"),
    ] {
        let statement = if original == "ValueError" {
            "raise ValueError()".to_owned()
        } else {
            format!("value = {original}((1, 2))")
        };
        for name in [original, replacement] {
            for prefix in ["", "*", "**"] {
                for declaration in ["def Generic", "class Generic"] {
                    let suffix = if declaration.starts_with("def") {
                        "()"
                    } else {
                        ""
                    };
                    let source = format!(
                        "{statement} # keep\n{declaration}[{prefix}{name}]{suffix}:\n    {statement}\n    def nested():\n        {statement}\n    values = [{original}((1, 2)) for item in (1,)]\n{statement} # keep\ndef outside():\n    {statement} # keep\n"
                    );
                    assert_plan(&source, operator, original, replacement).await;
                }
            }
        }
    }
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
        value = any(()) # keep
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

#[tokio::test]
async fn generic_headers_preserve_ordered_and_temporary_outer_bindings() {
    for source in [
        "class Early[T](list(items)): # keep\n    value = list(items) # keep\ntuple = object()\nclass Late[T](list(items)):\n    value = list(items)\n",
        "for item in items:\n    class C[T](list(items)):\n        value = list(items)\n    tuple = object()\n",
        "try:\n    work()\nexcept Exception as list:\n    class C[T](list(items)):\n        value = list(items)\nvalue = list(items) # keep\n",
        "class Outer:\n    try:\n        work()\n    except Exception as list:\n        class C[T](list(items)):\n            value = list(items) # keep\n",
    ] {
        assert_plan(source, "collection_list_tuple", "list", "tuple").await;
    }
}

#[tokio::test]
async fn generic_directives_and_local_overrides_preserve_lexical_rules() {
    for source in [
        "def generic[tuple]():\n    global tuple\n    return list(items) # keep\nclass C[tuple]:\n    global tuple\n    value = list(items) # keep\n",
        "def outer():\n    tuple = object()\n    def generic[T]():\n        nonlocal tuple\n        return list(items)\n    return list(items)\nvalue = list(items) # keep\n",
        "def generic[tuple]():\n    def inner():\n        global tuple\n        return list(items) # keep\n    return list(items)\nvalue = list(items) # keep\n",
        "def generic[T]():\n    value = list(items)\n    tuple = object()\nclass C[T]:\n    value = list(items) # keep\n    tuple = object()\n    later = list(items)\n    def method(self):\n        return list(items) # keep\n",
    ] {
        assert_plan(source, "collection_list_tuple", "list", "tuple").await;
    }
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
    value = any(items) # keep
    def method(self):
        return any(items) # keep
value = any(items) # keep
",
        "collection_any_all",
        "any",
        "all",
    )
    .await;
    assert_plan("def f[tuple](arg=42, *, kw=list(items)): # keep\n    return list(items)\nclass C[T](marker=list(items)): # keep\n    value = list(items) # keep\n", "collection_list_tuple", "list", "tuple").await;
}
