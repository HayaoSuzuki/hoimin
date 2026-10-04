use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn python_executable() -> PathBuf {
    env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            if cfg!(windows) {
                repo_root().join(".venv/Scripts/python.exe")
            } else {
                repo_root().join(".venv/bin/python")
            }
        },
        PathBuf::from,
    )
}

async fn plan(root: &Path, operator: &str) -> serde_json::Value {
    let python = python_executable();
    let args = [
        OsString::from("hoimin"),
        OsString::from("plan"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("subject.py"),
        OsString::from("--operators"),
        OsString::from(operator),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.into_os_string(),
        OsString::from("-c"),
        OsString::from("pass"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    serde_json::from_slice(&stdout).expect("plan writes one JSON document")
}

fn run_python(root: &Path, harness: &str) -> Output {
    Command::new(python_executable())
        .args(["-c", harness])
        .current_dir(root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("controlled Python interpreter runs")
}

fn compile(root: &Path, source: &str) {
    fs::write(root.join("compile.py"), source).unwrap();
    let result = run_python(
        root,
        "from pathlib import Path; compile(Path('compile.py').read_bytes(), 'subject.py', 'exec')",
    );
    assert!(
        result.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[tokio::test]
async fn mapping_key_candidates_compile() {
    for (keys, expected) in [
        ("True: a, False: b", 0),
        ("1+2j: a, 1-2j: b", 0),
        ("False: a, 1: b", 0),
        ("1.0: a, False: b", 0),
        ("True: a, -0.0: b", 0),
        ("-0: a, True: b", 0),
        ("True: a, -0j: b", 0),
        ("False: a, 1+0j: b", 1),
        ("True: a, -0.0-0j: b", 1),
        ("-1+2j: a, -1-2j: b", 0),
        ("-0.0+2j: a, 0.0-2j: b", 0),
        ("1e999+2j: a, 1e999-2j: b", 0),
        ("-1e999+2j: a, -1e999-2j: b", 0),
        ("9007199254740993+1j: a, 9007199254740992-1j: b", 0),
        ("0x20_0000_0000_0001+1j: a, 9007199254740992-1j: b", 0),
        ("18446744073709551617+1j: a, 18446744073709551616-1j: b", 0),
        (
            "0x1_0000_0000_0000_0001+1j: a, 18446744073709551616-1j: b",
            0,
        ),
        (
            "0o2_000000000000000000001+1j: a, 18446744073709551616-1j: b",
            0,
        ),
        (
            "0b1_0000000000000000000000000000000000000000000000000000000000000001+1j: a, 18446744073709551616-1j: b",
            0,
        ),
        ("18446744073709553664+1j: a, 18446744073709551616-1j: b", 0),
        ("18446744073709557760+1j: a, 18446744073709559808-1j: b", 0),
        ("1+1e999j: a, 1-1e999j: b", 0),
        ("0+2j: a, -2j: b", 0),
        ("True: a, 'False': b", 1),
        ("False: a, constants.ONE: b", 1),
        ("True: a", 1),
        ("False: a, 2: b", 1),
        ("True: a, -1: b", 1),
        ("True: a, 18446744073709551617: b", 1),
        ("1+2j: a", 1),
        ("1+2j: a, 2-2j: b", 2),
        ("1+0j: a, 2: b", 1),
        ("9007199254740993+0j: a, 9007199254740993: b", 1),
        ("9007199254740993+1j: a, 9007199254740994-1j: b", 2),
        ("18446744073709553665+1j: a, 18446744073709551616-1j: b", 2),
        ("True: {False: a}, 2: b", 2),
        ("True: a, 2: False", 2),
        ("True: a, 2: 1+2j", 2),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source =
            format!("def classify(value):\n    match value:\n        case {{{keys}}}: return a\n");
        compile(dir.path(), &source);
        fs::write(dir.path().join("subject.py"), &source).unwrap();
        let result = plan(dir.path(), "boolean_literal,binary_add_sub").await;
        let candidates = result["candidates"].as_array().unwrap();
        for candidate in candidates {
            let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
            let end =
                start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
            assert_eq!(&source[start..end], candidate["original"].as_str().unwrap());
            let mut mutant = source.clone();
            mutant.replace_range(start..end, candidate["replacement"].as_str().unwrap());
            compile(dir.path(), &mutant);
        }
        assert_eq!(candidates.len(), expected, "{keys}");
    }
}

#[tokio::test]
async fn adjacent_patterns_and_dictionary_expressions_keep_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let source = "def classify(value):\n    match value:\n        case {True: a}: return a\n        case {False: b}: return b\ndictionary = {True: 1, False: 2, 1+2j: 3, 1-2j: 4}\n";
    compile(dir.path(), source);
    fs::write(dir.path().join("subject.py"), source).unwrap();
    let result = plan(dir.path(), "boolean_literal,binary_add_sub").await;
    let candidates = result["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 6);
    for candidate in candidates {
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, candidate["replacement"].as_str().unwrap());
        compile(dir.path(), &mutant);
    }
}

#[tokio::test]
async fn import_only_run_does_not_count_duplicate_key_kills() {
    for (keys, survived) in [
        ("True: a, False: b", 0),
        ("1+2j: a, 1-2j: b", 0),
        ("False: a, 2: b", 1),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source =
            format!("def classify(value):\n    match value:\n        case {{{keys}}}: return a\n");
        fs::write(dir.path().join("subject.py"), &source).unwrap();
        assert!(run_python(dir.path(), "import subject").status.success());
        let output = Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--root"])
            .arg(dir.path())
            .args([
                "--file",
                "subject.py",
                "--operators",
                "boolean_literal,binary_add_sub",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1MiB",
                "--",
            ])
            .arg(python_executable())
            .args(["-c", "import subject"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(i32::from(survived > 0)),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["summary"]["counts"]["killed"], 0, "{report}");
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["complete"], true);
        assert_eq!(report["mutants"].as_array().unwrap().len(), survived);
        assert_eq!(report["summary"]["counts"]["survived"], survived);
        assert_eq!(
            fs::read_to_string(dir.path().join("subject.py")).unwrap(),
            source
        );
    }
}

#[tokio::test]
async fn retained_candidate_ids_spans_and_selection_are_stable() {
    let dir = tempfile::tempdir().unwrap();
    let source = "# é\r\ndef classify(value):\r\n    match value:\r\n        case {True: _, False: _}: return True\r\n        case {2+3j: _, 4: _}: return False\r\n";
    compile(dir.path(), source);
    fs::write(dir.path().join("subject.py"), source).unwrap();
    let all = plan(dir.path(), "boolean_literal,binary_add_sub").await;
    let candidates = all["candidates"].as_array().unwrap();
    let expected = [
        (
            source.find("return True").unwrap() + 7,
            "True",
            "False",
            "boolean_literal",
        ),
        (
            source.find("return False").unwrap() + 7,
            "False",
            "True",
            "boolean_literal",
        ),
        (source.find("+3j").unwrap(), "+", "-", "binary_add_sub"),
    ];
    assert_eq!(candidates.len(), expected.len());
    for (candidate, (start, original, replacement, operator)) in candidates.iter().zip(expected) {
        assert_eq!(candidate["span"]["start"], start);
        assert_eq!(candidate["span"]["length"], original.len());
        assert_eq!(candidate["original"], original);
        assert_eq!(candidate["replacement"], replacement);
        assert_eq!(candidate["operator"], operator);
        assert_ne!(candidate["id"].as_str().unwrap(), "");
        let mut mutant = source.to_owned();
        mutant.replace_range(start..start + original.len(), replacement);
        compile(dir.path(), &mutant);
    }
    for operator in ["boolean_literal", "binary_add_sub"] {
        let selected = plan(dir.path(), operator).await;
        let expected: Vec<_> = candidates
            .iter()
            .filter(|candidate| candidate["operator"] == operator)
            .cloned()
            .collect();
        let selected = selected["candidates"].as_array().unwrap();
        assert_eq!(selected.len(), expected.len());
        for (selected, expected) in selected.iter().zip(expected) {
            // Rank and discovery sequence depend on the selected candidate set.
            for field in [
                "id",
                "span",
                "original",
                "replacement",
                "operator",
                "line",
                "column",
                "symbol",
            ] {
                assert_eq!(selected[field], expected[field], "{field}");
            }
        }
    }
    assert_eq!(
        plan(dir.path(), "boolean_literal,binary_add_sub").await["candidates"],
        all["candidates"]
    );
}

#[tokio::test]
async fn finite_complex_real_digit_boundaries_remain_indexed() {
    for real in [
        format!("1{}", "0".repeat(308)),
        format!("0x_0008{}", "0".repeat(255)),
        format!("0o1{}", "0".repeat(341)),
        format!("0b1{}", "0".repeat(1023)),
    ] {
        let dir = tempfile::tempdir().unwrap();
        for (keys, expected) in [
            (format!("{real}+1j: _, {real}-1j: _"), 0),
            (format!("{real}+1j: _"), 1),
        ] {
            let source =
                format!("def classify(value):\n    match value:\n        case {{{keys}}}: pass\n");
            compile(dir.path(), &source);
            fs::write(dir.path().join("subject.py"), &source).unwrap();
            let result = plan(dir.path(), "binary_add_sub").await;
            let candidates = result["candidates"].as_array().unwrap();
            assert_eq!(candidates.len(), expected, "{keys}");
            for candidate in candidates {
                let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
                let mut mutant = source.clone();
                let length =
                    usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
                mutant.replace_range(
                    start..start + length,
                    candidate["replacement"].as_str().unwrap(),
                );
                compile(dir.path(), &mutant);
            }
        }
    }
}
