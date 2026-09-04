use std::ffi::OsString;

fn successful_command() -> Vec<OsString> {
    #[cfg(unix)]
    {
        vec![OsString::from("/usr/bin/true")]
    }
    #[cfg(windows)]
    {
        vec![
            OsString::from("cmd.exe"),
            OsString::from("/C"),
            OsString::from("exit 0"),
        ]
    }
}

fn with_host_independent_reserve(mut args: Vec<OsString>) -> Vec<OsString> {
    assert!(
        !args.iter().any(|argument| argument == "--min-free-space"),
        "host-independent runtime test must not override an explicit reserve"
    );
    let command_separator = args
        .iter()
        .position(|argument| argument == "--")
        .expect("runtime test arguments contain a command separator");
    args.splice(
        command_separator..command_separator,
        [OsString::from("--min-free-space"), OsString::from("1B")],
    );
    args
}

#[tokio::test]
async fn public_runtime_emits_measured_disk_and_cleanup_evidence() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
    let mut args = with_host_independent_reserve(vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("target.py"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--max-mutants"),
        OsString::from("1"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
    ]);
    args.extend(successful_command());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert!(
        matches!(exit, 0 | 1),
        "exit={exit}, stderr={:?}",
        String::from_utf8_lossy(&stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let disk = &report["summary"]["disk"];
    assert!(disk["sample_count"].as_u64().unwrap() >= 3, "{disk}");
    assert!(disk["removed_logical_bytes"].as_u64().is_some(), "{disk}");
    assert!(
        disk["filesystems"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(disk["cleanup"].as_array().unwrap().iter().any(|record| {
        record["root_id"] == "execution"
            && record["status"] == "clean"
            && record["removed_entries"]
                .as_u64()
                .is_some_and(|value| value > 0)
    }));
    assert!(disk["cleanup"].as_array().unwrap().iter().any(|record| {
        record["root_id"] == "delivery" && record["status"] == "cleanup_after_delivery"
    }));
}

#[tokio::test]
async fn public_initial_reserve_failure_uses_the_typed_report_path_before_dispatch() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("target.py"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--min-free-space"),
        OsString::from("18446744073709551615B"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
    ];
    args.extend(successful_command());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_ne!(exit, 0, "stderr={:?}", String::from_utf8_lossy(&stderr));
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(
        report["summary"]["disk"]["stop"]["code"],
        "filesystem.reserve.reached"
    );
    assert_eq!(report["summary"]["counts"]["killed"], 0);
    assert_eq!(report["summary"]["counts"]["survived"], 0);
}

#[tokio::test]
async fn public_jsonl_runtime_emits_disk_evidence() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
    let mut args = with_host_independent_reserve(vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("target.py"),
        OsString::from("--format"),
        OsString::from("jsonl"),
        OsString::from("--max-mutants"),
        OsString::from("1"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
    ]);
    args.extend(successful_command());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert!(
        matches!(exit, 0 | 1),
        "stderr={:?}",
        String::from_utf8_lossy(&stderr)
    );
    let finished = String::from_utf8(stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|event| event["kind"] == "run_finished")
        .expect("run_finished JSONL event");
    assert!(finished["disk"]["sample_count"].as_u64().unwrap() >= 3);
    assert!(finished["disk"]["cleanup"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|record| record["root_id"] == "execution" && record["status"] == "clean")
    }));
}

#[tokio::test]
async fn public_human_runtime_emits_disk_policy_evidence() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
    let mut args = with_host_independent_reserve(vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("target.py"),
        OsString::from("--format"),
        OsString::from("human"),
        OsString::from("--max-mutants"),
        OsString::from("1"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
    ]);
    args.extend(successful_command());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert!(
        matches!(exit, 0 | 1),
        "stderr={:?}",
        String::from_utf8_lossy(&stderr)
    );
    let stdout = String::from_utf8(stdout).unwrap();
    assert!(stdout.contains("disk max owned bytes:"), "{stdout}");
    assert!(stdout.contains("disk minimum free bytes:"), "{stdout}");
}
