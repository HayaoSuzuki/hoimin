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
async fn statement_deletion_preserves_suites_and_rejects_binding_changes() {
    for source in [
        "def save(store, record):\n    store.persist(record)\n",
        "if flag: call(); other = 1 # tail\n",
        "é = 1\ncall(\n    1, # inside\n) # tail\n",
        "class C:\n    call()\n",
        "(call()) # parentheses\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let document = plan(root.path(), "statement_delete", "pass").await;
        let candidates = document["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{source}");
        let candidate = &candidates[0];
        assert_eq!(candidate["replacement"], "pass");
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, "pass");
        compile(&mutant).await;
    }
    for source in [
        "value = call()\n",
        "def f():\n    return call()\n",
        "async def f():\n    call(await g())\n",
        "def f():\n    call((yield 1))\n",
        "call(lambda: (yield 1))\n",
        "def f():\n    call((yield from g()))\n",
        "call((x := 1))\n",
        "x: call()\n",
        "type T = call()\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let document = plan(root.path(), "statement_delete", "pass").await;
        assert_eq!(
            document["candidates"].as_array().unwrap().len(),
            0,
            "{source}"
        );
    }
}

#[tokio::test]
async fn statement_deletion_saved_plan_exposes_unobserved_persistence() {
    for (test, killed, survived) in [
        ("from subject import save; save([])", 0, 1),
        (
            "from subject import save; store = []; save(store); assert store == [7]",
            1,
            0,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = "def save(store):\n    store.append(7)\n";
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let document = plan(root.path(), "statement_delete", test).await;
        assert_eq!(document["candidates"].as_array().unwrap().len(), 1);
        let repeated = plan(root.path(), "statement_delete", test).await;
        assert_eq!(document["candidates"], repeated["candidates"]);
        let manifest = root.path().join("plan.json");
        std::fs::write(&manifest, serde_json::to_vec(&document).unwrap()).unwrap();
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
        assert_eq!(
            std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
            source
        );
    }
}

#[tokio::test]
async fn statement_deletion_preserves_encoded_bytes_and_newlines() {
    for newline in ["\n", "\r\n", "\r"] {
        for encoding in ["utf8", "bom", "latin1"] {
            let root = tempfile::tempdir().unwrap();
            let text = format!(
                "# coding: {}{newline}é = 1{newline}call(); other = 2 # tail{newline}",
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
            let document = plan(root.path(), "statement_delete", "pass").await;
            let candidates = document["candidates"].as_array().unwrap();
            assert_eq!(candidates.len(), 1, "{encoding} {newline:?}");
            let c = &candidates[0];
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
            assert_eq!(&bytes[start..end], b"call()");
            bytes.splice(start..end, b"pass".iter().copied());
            let result = tokio::process::Command::new(python()).args(["-c",
                "import json,sys; assert sys.version_info[:2] == (3,14); compile(bytes(json.loads(sys.argv[1])), '<encoded>', 'exec')",
                &serde_json::to_string(&bytes).unwrap()]).output().await.unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
fn statement_deletion_is_explicit_and_excludable() {
    use hoimin_core::{MutationOperator, MutationOperatorSelection};
    let operator = MutationOperator::from_name("statement_delete").unwrap();
    let mut selection = MutationOperatorSelection::default();
    assert!(!selection.contains(operator));
    selection.include(operator);
    assert!(selection.contains(operator));
    selection.exclude(operator);
    assert!(!selection.contains(operator));
}
