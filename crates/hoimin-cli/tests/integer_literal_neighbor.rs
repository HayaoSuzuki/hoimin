use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap();
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

async fn compile(source: &str) {
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args([
                "-c",
                "import sys; assert sys.version_info[:2] == (3, 14), sys.version; compile(sys.argv[1], '<neighbor>', 'exec')",
                source,
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython compiler deadline")
    .expect("CPython 3.14; set HOIMIN_OPERATOR_TEST_PYTHON to its executable");
    assert!(
        output.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn cli(args: Vec<OsString>) -> (i32, serde_json::Value) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert!(
        code <= 1,
        "exit={code}, stderr={}",
        String::from_utf8_lossy(&stderr)
    );
    (
        code,
        serde_json::from_slice(&stdout).expect("one JSON document"),
    )
}

async fn plan(root: &Path, operators: &str, test: &str) -> serde_json::Value {
    let args = [
        OsString::from("hoimin"),
        "plan".into(),
        "--root".into(),
        root.as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        operators.into(),
        "--min-free-space".into(),
        "1B".into(),
        "--baseline-timeout".into(),
        "10s".into(),
        "--mutant-timeout".into(),
        "10s".into(),
        "--total-timeout".into(),
        "30s".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python().into_os_string(),
        "-B".into(),
        "-c".into(),
        test.into(),
    ];
    let (code, document) = cli(args.into()).await;
    assert_eq!(code, 0);
    document
}

#[tokio::test]
async fn integer_neighbors_preserve_signed_values_precedence_and_bounds() {
    for (source, mut expected) in [
        ("def f():\n    return 3\n", vec!["(2)", "(4)"]),
        ("x = -3\n", vec!["(-4)", "(-2)"]),
        (
            "é = '名前'\r\nx = -(\r\n3 # comment\r\n)\r\n",
            vec!["(-4)", "(-2)"],
        ),
        ("x = 3 .real\n", vec!["(2)", "(4)"]),
        ("x = range(3)\n", vec!["(2)", "(4)"]),
        ("x = 0 ** y\n", vec!["(-1)", "(1)"]),
        ("x = (0).real\n", vec!["(-1)", "(1)"]),
        ("x = 18446744073709551615\n", vec!["(18446744073709551614)"]),
        (
            "x = -18446744073709551615\n",
            vec!["(-18446744073709551614)"],
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "integer_literal_neighbor", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        let mut actual: Vec<_> = candidates
            .iter()
            .map(|c| c["replacement"].as_str().unwrap())
            .collect();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected, "{source}");
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "integer_literal_neighbor", "pass").await["candidates"]
        );
        for c in candidates {
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
            let mut mutant = source.to_owned();
            mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
            compile(&mutant).await;
        }
    }
}

#[tokio::test]
async fn integer_neighbors_exclude_nonruntime_and_structural_slices() {
    for source in [
        "x = True\ny = '123' # 3\n",
        "x: Literal[3]\n",
        "type T = Literal[3]\n",
        "match x:\n    case 3: pass\n",
        "x = a[3 + 4]\n",
        "x = a[1:5:1]\n",
        "a[3] = 4.0\n",
        "(3).x = 4.0\n",
        "del (3).x\n",
        "x = a[f(3)][g(4):]\n",
        "x = 0x10\ny = 1_000\nz = 3j\n",
        "x = 18446744073709551616\n",
        "x = -18446744073709551616\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "integer_literal_neighbor", "pass").await;
        assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
    }
}

#[tokio::test]
async fn integer_neighbors_saved_plan_distinguishes_observed_constant() {
    for (test, killed, survived) in [
        ("from subject import count; count()", 0, 2),
        ("from subject import count; assert count() == 3", 2, 0),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("subject.py"),
            "def count():\n    return 3\n",
        )
        .unwrap();
        let doc = plan(root.path(), "integer_literal_neighbor", test).await;
        let manifest = root.path().join("plan.json");
        std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
        let (_, report) = cli(vec![
            "hoimin".into(),
            "verify".into(),
            manifest.into_os_string(),
            "--top".into(),
            "2".into(),
            "--format".into(),
            "json".into(),
        ])
        .await;
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["counts"]["killed"], killed);
        assert_eq!(report["summary"]["counts"]["survived"], survived);
    }
}
