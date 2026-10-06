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
async fn containers_delete_one_element_and_compile() {
    for (literal, count, assertion) in [
        ("[1,2,3]", 3, "assert value in ([2,3],[1,3],[1,2])"),
        (
            "(1,2)",
            2,
            "assert type(value) is tuple and value in ((1,), (2,))",
        ),
        (
            "{'a':1,'b':2,'c':3}",
            3,
            "assert type(value) is dict and len(value)==2",
        ),
        ("[1]", 1, "assert value == []"),
        ("(1,)", 1, "assert value == ()"),
        ("{'a':1}", 1, "assert value == {}"),
        ("[1,1]", 1, "assert value == [1]"),
        ("[[1,2],3]", 4, "assert type(value) is list"),
        (
            "[\n 'a,b', # comma\n (lambda: 'c,d')(),\n]",
            2,
            "assert value in (['a,b'], ['c,d'])",
        ),
    ] {
        let source = format!("é = 1\r\nvalue = {literal}\n");
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "container_element_delete", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), count, "{source}");
        for c in candidates {
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
            let mut mutant = source.clone();
            mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
            compile(&mutant).await;
            mutant.push_str(assertion);
            let output = tokio::process::Command::new(python())
                .args(["-c", &mutant])
                .output()
                .await
                .unwrap();
            assert!(
                output.status.success(),
                "{mutant}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "container_element_delete", "pass").await["candidates"]
        );
    }
}

#[tokio::test]
async fn containers_exclude_unsupported_and_nonruntime_roles() {
    for source in [
        "a=[]; b=(); c={}; d={1,2}; e=[x for x in xs]; f=1,2\n",
        "a=[*items,1]; b={**mapping,'a':1}\n",
        "a=[(x:=1),2]\n",
        "async def f():\n    return [await g(),1]\n",
        "def f():\n    return [(yield 1),2]\n",
        "[a,b]=items\na[[1,2]]=3\ndel a[(1,2)]\n",
        "from typing import TypeAlias, Literal\nA: TypeAlias = Literal[[1,2]]\nx: Literal[(1,2)]\ntype B = Literal[[3,4]]\n",
        "match x:\n    case [1,2]: pass\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        assert!(
            plan(root.path(), "container_element_delete", "pass").await["candidates"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{source}"
        );
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "x: list = [1,2]\ny = matrix[[3,4]]\n",
    )
    .unwrap();
    assert_eq!(
        plan(root.path(), "container_element_delete", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[tokio::test]
async fn containers_saved_plan_requires_all_payload_fields() {
    for (test, killed, survived) in [
        (
            "from subject import payload; assert payload(7)['user_id']==7",
            1,
            1,
        ),
        (
            "from subject import payload; assert payload(7)=={'user_id':7,'enabled':True}",
            2,
            0,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("subject.py"),
            "def payload(user_id):\n    return {'user_id':user_id,'enabled':True}\n",
        )
        .unwrap();
        let doc = plan(root.path(), "container_element_delete", test).await;
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

#[tokio::test]
async fn containers_remove_element_evaluation_once() {
    let source =
        "calls=[]\ndef mark(n):\n    calls.append(n)\n    return n\nvalue = [mark(1),mark(2)]\n";
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "container_element_delete", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    for c in candidates {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        mutant.push_str("assert calls == value and len(calls)==1\n");
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
}
