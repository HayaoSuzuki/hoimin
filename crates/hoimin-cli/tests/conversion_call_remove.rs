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
async fn conversions_cover_builtins_source_and_precedence() {
    for name in [
        "int",
        "float",
        "complex",
        "bool",
        "str",
        "bytes",
        "list",
        "tuple",
        "dict",
        "set",
        "frozenset",
    ] {
        let source = format!(
            "é = 1\r\ndef f(raw):\r\n    return {name}(\r\n        raw # comment\r\n    )\r\n"
        );
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "conversion_call_remove", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{name}");
        assert_eq!(candidates[0]["replacement"], "(raw)");
        let c = &candidates[0];
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.clone();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        compile(&mutant).await;
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "conversion_call_remove", "pass").await["candidates"]
        );
    }
    for source in [
        "value = 2 * int(1 + 2)\n",
        "value = int(1 if True else 2)\n",
        "value = list((1,2))\n",
        "value = str(lambda: 1)\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "conversion_call_remove", "pass").await;
        let c = &doc["candidates"][0];
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        compile(&mutant).await;
        if source.starts_with("value = 2") {
            mutant.push_str("assert value == 6\n");
            assert!(
                tokio::process::Command::new(python())
                    .args(["-c", &mutant])
                    .status()
                    .await
                    .unwrap()
                    .success()
            );
        }
    }
}

#[tokio::test]
async fn conversions_exclude_shadowing_unsafe_and_nonruntime_roles() {
    for source in [
        "int = custom\nx = int(raw)\n",
        "complex = custom\nx = complex(raw)\n",
        "def f(complex): return complex(raw)\n",
        "def rebind():\n    global complex\n    complex = custom\ndef f(): return complex(raw)\n",
        "def f(int): return int(raw)\n",
        "def f():\n    x = int(raw)\n    int = custom\n",
        "def rebind():\n    global int\n    int = custom\ndef f(): return int(raw)\n",
        "def outer():\n    int = custom\n    def inner():\n        nonlocal int\n        return int(raw)\n",
        "from somewhere import *\nx = int(raw)\n",
        "exec(code)\nx = int(raw)\n",
        "x = builtins.int(raw)\nx = int()\nx = int(raw, 10)\nx = int(raw, base=10)\nx = int(*args)\nx = int(**kw)\n",
        "x = list(x for x in xs)\nx = int([x for x in (y for y in ys)])\nx = int(a:=1)\n",
        "async def f(): return int(await g())\n",
        "def f(): return int((yield 1))\n",
        "from typing import TypeAlias\nA: TypeAlias = int(raw)\nx: int(raw)\ntype B = int(raw)\na[int(raw)] = 1\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        assert!(
            plan(root.path(), "conversion_call_remove", "pass").await["candidates"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{source}"
        );
    }
}

#[tokio::test]
async fn conversions_saved_plan_needs_string_input_and_type_check() {
    for (test, killed, survived) in [
        ("from subject import count; assert count(7)==7", 0, 1),
        (
            "from subject import count; value=count('7'); assert type(value) is int and value==7",
            1,
            0,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("subject.py"),
            "def count(raw): return int(raw)\n",
        )
        .unwrap();
        let doc = plan(root.path(), "conversion_call_remove", test).await;
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
async fn conversions_keep_one_argument_evaluation_and_distinct_legacy_edits() {
    for (source, assertion) in [
        (
            "calls=[]\nclass Box:\n    def __int__(self):\n        calls.append('convert')\n        return 7\ndef get():\n    calls.append('argument')\n    return Box()\nvalue = int(get())\n",
            "assert calls == ['argument'] and type(value) is Box\n",
        ),
        (
            "original={}\nvalue=dict(original)\nvalue['x']=1\n",
            "assert original == {'x':1} and value is original\n",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "conversion_call_remove", "pass").await;
        assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
        let c = &doc["candidates"][0];
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        mutant.push_str(assertion);
        let output = tokio::process::Command::new(python())
            .args(["-c", &mutant])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), "x = list(raw)\n").unwrap();
    let doc = plan(
        root.path(),
        "conversion_call_remove,collection_list_tuple",
        "pass",
    )
    .await;
    assert_eq!(doc["candidates"].as_array().unwrap().len(), 2);
    assert_ne!(
        doc["candidates"][0]["operator"],
        doc["candidates"][1]["operator"]
    );
}
