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
async fn negative_neighbor_plan_candidates_compile_with_cpython_314() {
    let root = tempfile::tempdir().unwrap();
    let source = concat!(
        "def boundaries(x):\n",
        "    return (x[-1], x[-1:], x[:-1], x[::-1], x[-2], x[::-2],\n",
        "            x[(-1)], x[-(1)], x[( -\n",
        "                (1) # operand comment\n",
        "            )], x[(\n",
        "                -1\n",
        "            )::(-2)])\n",
    );
    compile(source).await;
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let document = plan(
        root.path(),
        "structure_index_neighbor,structure_slice_neighbor",
        "pass",
    )
    .await;
    let candidates = document["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 21);
    for candidate in candidates {
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(&source[start..end], candidate["original"].as_str().unwrap());
        assert!(source[start..end].starts_with('-'));
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, candidate["replacement"].as_str().unwrap());
        compile(&mutant).await;
    }
    assert_eq!(
        std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
        source
    );
}

#[tokio::test]
async fn verify_negative_neighbor_detects_tail_boundary_missed_by_sign_flip() {
    let root = tempfile::tempdir().unwrap();
    let source = "def tail(x):\n    return x[-1]\n";
    compile(source).await;
    let subject = root.path().join("subject.py");
    std::fs::write(&subject, source).unwrap();
    // This deliberately weak boundary test cannot distinguish first/second/last,
    // but does distinguish the penultimate item from the last item.
    let test = "from subject import tail; assert tail([7, 7, 9, 7]) == 7";
    for (operator, expected_replacements, killed) in [
        ("unary_sign", vec!["+"], 0),
        ("structure_index_neighbor", vec!["-2", "0"], 1),
    ] {
        let document = plan(root.path(), operator, test).await;
        let candidates = document["candidates"].as_array().unwrap();
        let mut replacements: Vec<_> = candidates
            .iter()
            .map(|c| c["replacement"].as_str().unwrap())
            .collect();
        replacements.sort_unstable();
        assert_eq!(replacements, expected_replacements);
        let manifest = root.path().join("plan.json");
        std::fs::write(&manifest, serde_json::to_vec(&document).unwrap()).unwrap();
        let (code, report) = cli(vec![
            "hoimin".into(),
            "verify".into(),
            manifest.into_os_string(),
            "--top".into(),
            candidates.len().to_string().into(),
            "--format".into(),
            "json".into(),
        ])
        .await;
        assert_eq!(code, 1, "one surviving mutant should set exit 1");
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["counts"]["killed"], killed);
        assert_eq!(report["summary"]["counts"]["survived"], 1);
        assert_eq!(
            report["mutants"].as_array().unwrap().len(),
            candidates.len()
        );
        if killed == 1 {
            let mutant = report["mutants"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["status"] == "killed")
                .unwrap();
            assert_eq!(mutant["candidate"]["replacement"], "-2");
        }
        assert_eq!(std::fs::read_to_string(&subject).unwrap(), source);
    }
}
