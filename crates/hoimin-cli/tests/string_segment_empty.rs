use std::{ffi::OsString, path::PathBuf};

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(if cfg!(windows) {
                    ".venv/Scripts/python.exe"
                } else {
                    ".venv/bin/python"
                })
        },
        PathBuf::from,
    )
}

async fn cli(arguments: Vec<OsString>) -> serde_json::Value {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(arguments, &mut stdout, &mut stderr).await;
    assert!(
        code <= 1,
        "exit={code}: {}",
        String::from_utf8_lossy(&stderr)
    );
    serde_json::from_slice(&stdout).unwrap()
}

async fn plan(root: &std::path::Path, selection: &[&str], test: &str) -> serde_json::Value {
    let mut args: Vec<OsString> = ["hoimin", "plan", "--root"]
        .into_iter()
        .map(Into::into)
        .collect();
    args.push(root.as_os_str().to_owned());
    args.extend(selection.iter().map(OsString::from));
    args.extend(
        [
            "--file",
            "subject.py",
            "--min-free-space",
            "1B",
            "--baseline-timeout",
            "10s",
            "--mutant-timeout",
            "10s",
            "--total-timeout",
            "30s",
            "--allow-best-effort-memory",
            "--",
        ]
        .into_iter()
        .map(OsString::from),
    );
    args.extend([
        python().into_os_string(),
        "-B".into(),
        "-c".into(),
        test.into(),
    ]);
    cli(args).await
}

fn mutated(source: &str, candidate: &serde_json::Value) -> String {
    let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
    let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
    assert_eq!(
        &source[start..start + length],
        candidate["original"].as_str().unwrap()
    );
    let mut result = source.to_owned();
    result.replace_range(
        start..start + length,
        candidate["replacement"].as_str().unwrap(),
    );
    result
}

async fn execute(source: &str, assertion: &str) {
    let mut child = tokio::process::Command::new(python());
    child.kill_on_drop(true).args([
        "-B", "-c",
        "import sys; assert sys.version_info[:2]==(3,14); ns={}; exec(compile(sys.argv[1],'<mutant>','exec'),ns); exec(sys.argv[2],ns)",
        source, assertion,
    ]);
    let output = tokio::time::timeout(std::time::Duration::from_secs(15), child.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn segments_default_to_enabled_and_allow_explicit_exclusion() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "value = f'prefix:{x}:suffix'\n",
    )
    .unwrap();
    let doc = plan(root.path(), &[], "pass").await;
    let segments: Vec<_> = doc["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["operator"] == "string_segment_empty")
        .collect();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0]["original"], "prefix:");
    assert_eq!(segments[0]["replacement"], "");
    for selection in [
        vec!["--exclude-operators", "string_segment_empty"],
        vec!["--operators", "string_literal_empty"],
    ] {
        assert!(
            plan(root.path(), &selection, "pass").await["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["operator"] != "string_segment_empty")
        );
    }
}

#[tokio::test]
async fn segments_keep_interpolation_evaluation_and_formatting() {
    let prelude = "events=[]\ndef mark(x):\n events.append(x)\n return x\n";
    for (expression, original, expected) in [
        (
            "f'prefix:{mark(1)}:suffix{mark(2)}'",
            "prefix:",
            "1:suffix2",
        ),
        ("f'{mark(1)}:suffix'", ":suffix", "1"),
        ("fr'\\n{{前}}{mark(1)}:suffix'", "\\n{{前}}", "1:suffix"),
        ("f'''前\\n{mark(1)}後'''", "前\\n", "1後"),
        ("f'\\N{SNOWMAN}{{}}{mark(1)}'", "\\N{SNOWMAN}{{}}", "1"),
        ("f'prefix:{mark(1)!r:>{mark(2)}}'", "prefix:", " 1"),
        ("f'prefix:{mark(1)=}:suffix'", "prefix:", "mark(1)=1:suffix"),
        ("f'{mark(1)=}:suffix'", ":suffix", "mark(1)=1"),
        ("f'{{{mark(1)}}}'", "{{", "1}"),
        ("f'{mark(1)}{{}}'", "{{}}", "1"),
        ("f'\\\"{mark(1)}'", "\\\"", "1"),
        ("f'\\\n{mark(1)}:suffix'", ":suffix", "1"),
        ("'prefix' f'{mark(1)}:suffix'", "'prefix'", "1:suffix"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = format!("{prelude}result = {expression}\n");
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(
            root.path(),
            &["--operators", "string_segment_empty"],
            "pass",
        )
        .await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{expression}: {doc}");
        assert_eq!(candidates[0]["original"], original, "{expression}");
        let events = if expression.contains("mark(2)") {
            "[1,2]"
        } else {
            "[1]"
        };
        execute(
            &mutated(&source, &candidates[0]),
            &format!(
                "assert result == {}, repr(result); assert events == {events}, events",
                serde_json::to_string(expected).unwrap()
            ),
        )
        .await;
        assert_eq!(
            doc["candidates"],
            plan(
                root.path(),
                &["--operators", "string_segment_empty"],
                "pass"
            )
            .await["candidates"]
        );
    }
}

#[tokio::test]
async fn segments_empty_one_nonempty_concat_piece_and_keep_comments() {
    for expression in [
        "'' r'prefix' 'suffix'",
        "('prefix' # keep\n 'suffix')",
        "u'前' '''後'''",
        "'\\\n' 'prefix' 'suffix'",
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = format!("result = {expression}\n");
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(
            root.path(),
            &["--operators", "string_segment_empty"],
            "pass",
        )
        .await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{doc}");
        let result = mutated(&source, &candidates[0]);
        if source.contains("# keep") {
            assert!(result.contains("# keep"));
        }
        execute(
            &result,
            if expression.contains('前') {
                "assert result == '後'"
            } else {
                "assert result == 'suffix'"
            },
        )
        .await;
    }
}

#[tokio::test]
async fn segment_emptying_keeps_adjacent_quote_token_boundaries() {
    for expression in [
        "'prefix'\"suffix\"",
        "'prefix'\"\"\"suffix\"\"\"",
        "'prefix''suffix'",
        "\"\"'prefix'",
        "''\"prefix\"'suffix'",
        "f'prefix''suffix'",
        "rf'prefix'\"suffix\"",
        "f'''prefix'''\"\"\"suffix\"\"\"",
        "'prefix'f'{1}'",
        "''f'prefix''suffix'",
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = format!("result={expression}\n");
        execute(&source, "assert isinstance(result, str)").await;
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(
            root.path(),
            &["--operators", "string_segment_empty"],
            "pass",
        )
        .await;
        let cs = doc["candidates"].as_array().unwrap();
        assert_eq!(cs.len(), 1, "{doc}");
        execute(
            &mutated(&source, &cs[0]),
            if expression == "\"\"'prefix'" {
                "assert result == ''"
            } else if expression == "'prefix'f'{1}'" {
                "assert result == '1'"
            } else {
                "assert result == 'suffix', repr(result)"
            },
        )
        .await;
        // The same token spellings must also remain valid as a bare expression.
        let bare = format!("{expression}\n");
        std::fs::write(root.path().join("subject.py"), &bare).unwrap();
        // Ordinary bare strings are docstrings, so use a later expression statement.
        let bare = format!("pass\n{bare}");
        std::fs::write(root.path().join("subject.py"), &bare).unwrap();
        let doc = plan(
            root.path(),
            &["--operators", "string_segment_empty"],
            "pass",
        )
        .await;
        execute(&mutated(&bare, &doc["candidates"][0]), "pass").await;
    }
}

#[tokio::test]
async fn segments_exclude_non_runtime_roles_and_interpolation_contents() {
    let root = tempfile::tempdir().unwrap();
    let source = r#""module" "docstring"
from typing import TypeAlias
Alias: TypeAlias = "Type" "Name"
type Other = "Type" "Name"
annotation: "Type" "Name"
empty = '' ''
plain = 'single'
data = b'prefix' b'suffix'
template = t'prefix:{value}'
nested = f'{f"inner:{value}"}'
spec = f'{value:{f"inner:{width}"}}'
debug = f'{value=}'
def f(arg: "Type" "Name") -> "Type" "Name":
    "function" "docstring"
    match arg:
        case "pattern" "text": pass
class C:
    "class" "docstring"
"#;
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    assert_eq!(
        plan(
            root.path(),
            &["--operators", "string_segment_empty"],
            "pass"
        )
        .await["candidates"]
            .as_array()
            .unwrap(),
        &Vec::<serde_json::Value>::new()
    );
}

#[tokio::test]
async fn segments_preserve_conversion_and_dynamic_format_call_order() {
    let root = tempfile::tempdir().unwrap();
    let source = r"events=[]
class Value:
    def __repr__(self):
        events.append('repr')
        return 'é'
    def __str__(self):
        events.append('str')
        return 'é'
    def __format__(self, spec):
        events.append(('format', spec))
        return 'formatted'
def value():
    events.append('value')
    return Value()
def width():
    events.append('width')
    return 4
result = f'prefix:{value()!r:>{width()}}/{value()!s}/{value()!a}/{value():>{width()}}'
";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(
        root.path(),
        &["--operators", "string_segment_empty"],
        "pass",
    )
    .await;
    let cs = doc["candidates"].as_array().unwrap();
    assert_eq!(cs.len(), 1);
    execute(&mutated(source, &cs[0]), r"assert result == '   é/é/\\xe9/formatted', repr(result)
assert events == ['value', 'repr', 'width', 'value', 'str', 'value', 'repr', 'value', 'width', ('format', '>4')], events").await;
}

#[tokio::test]
async fn segment_saved_plans_distinguish_strong_and_weak_assertions() {
    for expression in ["f'prefix:{1}'", "'prefix:' '1'"] {
        for (test, outcome) in [
            (
                "from subject import state; assert state() == 'prefix:1'",
                "killed",
            ),
            (
                "from subject import state; assert state().endswith('1')",
                "survived",
            ),
        ] {
            let root = tempfile::tempdir().unwrap();
            let source = format!("def state():\n return {expression}\n");
            std::fs::write(root.path().join("subject.py"), &source).unwrap();
            let doc = plan(root.path(), &["--operators", "string_segment_empty"], test).await;
            let path = root.path().join("plan.json");
            std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
            let report = cli(vec![
                "hoimin".into(),
                "verify".into(),
                path.into_os_string(),
                "--top".into(),
                "1".into(),
                "--format".into(),
                "json".into(),
            ])
            .await;
            assert_eq!(report["baseline"]["termination"]["Exit"], 0);
            assert_eq!(report["summary"]["counts"][outcome], 1);
            assert_eq!(
                std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
                source
            );
        }
    }
}

#[tokio::test]
async fn segment_spans_use_original_encoding_and_newlines() {
    for newline in ["\n", "\r\n", "\r"] {
        for encoding in ["utf8", "bom", "latin1"] {
            let root = tempfile::tempdir().unwrap();
            let text = format!(
                "# coding: {}{newline}# é{newline}value=1{newline}result=f'é{{value}}:suffix'{newline}",
                if encoding == "latin1" {
                    "latin-1"
                } else {
                    "utf-8"
                }
            );
            let mut bytes = if encoding == "latin1" {
                text.chars()
                    .map(|c| u8::try_from(u32::from(c)).unwrap())
                    .collect::<Vec<_>>()
            } else {
                text.as_bytes().to_vec()
            };
            if encoding == "bom" {
                bytes.splice(0..0, [0xef, 0xbb, 0xbf]);
            }
            std::fs::write(root.path().join("subject.py"), &bytes).unwrap();
            let doc = plan(
                root.path(),
                &["--operators", "string_segment_empty"],
                "pass",
            )
            .await;
            let cs = doc["candidates"].as_array().unwrap();
            assert_eq!(cs.len(), 1, "{doc}");
            let start = usize::try_from(cs[0]["span"]["start"].as_u64().unwrap()).unwrap();
            let len = usize::try_from(cs[0]["span"]["length"].as_u64().unwrap()).unwrap();
            assert_eq!(
                &bytes[start..start + len],
                if encoding == "latin1" {
                    b"\xe9".as_slice()
                } else {
                    "é".as_bytes()
                }
            );
            bytes.drain(start..start + len);
            let code = format!(
                "import json; ns={{}}; exec(compile(bytes(json.loads({})), '<encoded>', 'exec'),ns); assert ns['result']=='1:suffix'",
                serde_json::to_string(&serde_json::to_string(&bytes).unwrap()).unwrap()
            );
            execute("", &code).await;
        }
    }
}

#[tokio::test]
async fn segment_plan_rejects_changed_source_and_invalid_offsets() {
    for changed_source in [false, true] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), "result=f'prefix:{1}'\n").unwrap();
        let mut doc = plan(
            root.path(),
            &["--operators", "string_segment_empty"],
            "pass",
        )
        .await;
        if changed_source {
            std::fs::write(root.path().join("subject.py"), "result=f'changed:{1}'\n").unwrap();
        } else {
            doc["candidates"][0]["span"]["start"] = 0.into();
        }
        let path = root.path().join("plan.json");
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = hoimin_cli::run_with_io(
            vec![
                "hoimin".into(),
                "verify".into(),
                path.into_os_string(),
                "--top".into(),
                "1".into(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;
        assert_eq!(code, 2, "{}", String::from_utf8_lossy(&stderr));
        assert!(
            String::from_utf8_lossy(&stderr).contains(if changed_source {
                "plan.source.changed"
            } else {
                "plan.candidate.invalid"
            })
        );
    }
}
