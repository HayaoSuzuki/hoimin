use std::path::PathBuf;
use std::time::Duration;

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
                "../../.venv/Scripts/python.exe"
            } else {
                "../../.venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

async fn output(mut command: tokio::process::Command) -> std::process::Output {
    command.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("child deadline")
        .expect("child starts");
    assert!(
        output.status.success(),
        "status={}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn corpus() -> impl Iterator<Item = serde_json::Value> {
    include_str!("../../../formal/HoiminOracle/corpus/with-suppression.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
}

#[tokio::test]
async fn with_suppression_public_plan_matches_runtime_and_corpus() {
    let custom = concat!(
        "class Suppress:\n",
        "    def __enter__(self): return self\n",
        "    def __exit__(self, *error): return True\n",
        "    async def __aenter__(self): return self\n",
        "    async def __aexit__(self, *error): return True\n",
    );
    let signature =
        "def record(value: Sequence[int]): pass\nobserved = record.__annotations__['value']\n";
    let additional = [
        serde_json::json!({
            "id": "custom_manager", "candidate_count": 0, "runtime_typing": [true, false],
            "source": format!("{custom}Sequence = set\nwith Suppress():\n    hazard()\n    from typing import Sequence\n{signature}")
        }),
        serde_json::json!({
            "id": "custom_manager_positive", "candidate_count": 1, "runtime_typing": [true, true],
            "source": format!("{custom}Sequence = set\nwith Suppress():\n    from typing import Sequence\n    hazard()\n{signature}")
        }),
        serde_json::json!({
            "id": "second_manager_entry", "candidate_count": 0, "runtime_typing": [true, false],
            "source": format!("{custom}def second():\n    hazard()\n    return Suppress()\nSequence = set\nwith Suppress(), second():\n    from typing import Sequence\n{signature}")
        }),
        serde_json::json!({
            "id": "async_manager", "candidate_count": 0, "runtime_typing": [true, false],
            "source": format!("{custom}import asyncio\nasync def f():\n    Sequence = set\n    async with Suppress():\n        hazard()\n        from typing import Sequence\n    def record(value: Sequence[int]): pass\n    return record.__annotations__['value']\nobserved = asyncio.run(f())\n")
        }),
        serde_json::json!({
            "id": "async_manager_positive", "candidate_count": 1, "runtime_typing": [true, true],
            "source": format!("{custom}import asyncio\nasync def f():\n    Sequence = set\n    async with Suppress():\n        from typing import Sequence\n        hazard()\n    def record(value: Sequence[int]): pass\n    return record.__annotations__['value']\nobserved = asyncio.run(f())\n")
        }),
    ];
    for case in corpus().chain(additional) {
        let directory = tempfile::tempdir().unwrap();
        let source = case["source"].as_str().unwrap();
        std::fs::write(directory.path().join("subject.py"), source).unwrap();
        let script = concat!(
            "import collections.abc, json, sys, typing\n",
            "assert sys.version_info >= (3, 14), sys.version\n",
            "source = open('subject.py').read()\n",
            "results = []\n",
            "for raises in (False, True):\n",
            "    def hazard():\n",
            "        if raises: raise KeyError()\n",
            "    ns = {'hazard': hazard}\n",
            "    exec(compile(source, 'subject.py', 'exec'), ns)\n",
            "    results.append(typing.get_origin(ns['observed']) is collections.abc.Sequence)\n",
            "print(json.dumps(results))\n",
        );
        let mut runtime = tokio::process::Command::new(python());
        runtime
            .args(["-B", "-c", script])
            .current_dir(directory.path());
        let runtime = output(runtime).await;
        let observed: serde_json::Value = serde_json::from_slice(&runtime.stdout).unwrap();
        assert_eq!(observed, case["runtime_typing"], "{}", case["id"]);
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(["plan", "--root"])
            .arg(directory.path())
            .args([
                "--file",
                "subject.py",
                "--operators",
                "type_list_sequence",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--",
            ])
            .arg(python())
            .args(["-c", "pass"]);
        let result = output(command).await;
        let plan: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            plan["candidates"].as_array().unwrap().len() as u64,
            case["candidate_count"].as_u64().unwrap(),
            "{}\n{plan}",
            case["id"]
        );
    }
}

#[tokio::test]
async fn with_suppression_public_run_does_not_count_invalid_kill() {
    let directory = tempfile::tempdir().unwrap();
    let case = corpus().next().unwrap();
    std::fs::write(
        directory.path().join("subject.py"),
        case["source"].as_str().unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("check.py"),
        concat!(
            "def hazard():\n    raise KeyError()\n",
            "ns = {'hazard': hazard}\n",
            "exec(compile(open('subject.py').read(), 'subject.py', 'exec'), ns)\n",
            "assert ns['observed'] == set[int]\n",
        ),
    )
    .unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["run", "--root"])
        .arg(directory.path())
        .args([
            "--file",
            "subject.py",
            "--operators",
            "type_list_sequence",
            "--format",
            "json",
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
            "--",
        ])
        .arg(python())
        .args(["-B", "check.py"]);
    let result = output(command).await;
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0, "{report}");
    assert_eq!(report["summary"]["counts"]["killed"], 0, "{report}");
    assert_eq!(report["summary"]["complete"], true, "{report}");
    assert!(report["mutants"].as_array().unwrap().is_empty(), "{report}");
}
