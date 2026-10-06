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
async fn augmented_assignment_replaces_only_every_operator_token() {
    for op in [
        "+=", "-=", "*=", "/=", "//=", "%=", "**=", "@=", "&=", "|=", "^=", "<<=", ">>=",
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = format!(
            "é = '前'\r\ndef f(total, amount):\r\n    (total) {op} (\r\n        amount # += not a token\r\n    ); return total\r\n"
        );
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "augmented_to_assignment", "pass").await;
        let cs = doc["candidates"].as_array().unwrap();
        assert_eq!(cs.len(), 1, "{doc}");
        let c = &cs[0];
        assert_eq!(c["original"], op);
        assert_eq!(c["replacement"], "=");
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        assert_eq!(&source[start..start + op.len()], op);
        let mutant = format!("{}={}", &source[..start], &source[start + op.len()..]);
        compile(&mutant).await;
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "augmented_to_assignment", "pass").await["candidates"]
        );
    }
}

#[tokio::test]
async fn augmented_assignment_excludes_complex_targets_and_coexists() {
    let root = tempfile::tempdir().unwrap();
    let source = "obj.total += amount\nitems[i] += amount\ntotal = amount\ns = 'total += amount' # total += amount\ndef f(total, amount):\n    total += amount\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "augmented_to_assignment", "pass").await;
    assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(doc["candidates"][0]["symbol"], "f");
    let doc = plan(
        root.path(),
        "augmented_to_assignment,augmented_add_sub",
        "pass",
    )
    .await;
    let cs = doc["candidates"].as_array().unwrap();
    assert_eq!(cs.len(), 4);
    let selected: Vec<_> = cs.iter().filter(|c| c["symbol"] == "f").collect();
    assert_eq!(selected.len(), 2);
    assert!(selected.iter().any(|c| c["replacement"] == "="));
    assert!(selected.iter().any(|c| c["replacement"] == "-="));
}

#[tokio::test]
async fn augmented_assignment_accumulation_requires_multiple_elements() {
    for (test, killed, survived) in [
        ("from subject import total; assert total([3])==3", 0, 1),
        ("from subject import total; assert total([3,5])==8", 1, 0),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"),"def total(values):\n    result=0\n    for value in values:\n        result += value\n    return result\n").unwrap();
        let doc = plan(root.path(), "augmented_to_assignment", test).await;
        let manifest = root.path().join("plan.json");
        std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
        let (_, report) = cli(vec![
            "hoimin".into(),
            "verify".into(),
            manifest.into_os_string(),
            "--top".into(),
            "1".into(),
            "--format".into(),
            "json".into(),
        ])
        .await;
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["counts"]["killed"], killed);
        assert_eq!(report["summary"]["counts"]["survived"], survived);
    }
}

#[tokio::test]
async fn augmented_assignment_removes_in_place_effects_for_non_numeric_values() {
    let root = tempfile::tempdir().unwrap();
    let source = "def append(values, extra):\n    values += extra\n    return values\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "augmented_to_assignment", "pass").await;
    let c = &doc["candidates"][0];
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let mutant = format!("{}={}", &source[..start], &source[start + 2..]);
    for (text, assertion) in [
        (source, "assert original==[1,2] and result is original"),
        (mutant.as_str(), "assert original==[1] and result is extra"),
    ] {
        let script = format!(
            "ns={{}}; exec({text:?},ns); original=[1]; extra=[2]; result=ns['append'](original,extra); {assertion}"
        );
        let output = tokio::process::Command::new(python())
            .args(["-c", &script])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
