use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

const SOURCE: &str = "first = 1 + 2\nsecond = 3 + 4\n";

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        })
}

async fn cli(args: &[String]) -> Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command.args(args).kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("bounded boundary fixture CLI")
        .unwrap()
}

fn source_args(command: &str, root: &Path, limit: usize) -> Vec<String> {
    vec![
        command.into(),
        "--root".into(),
        root.display().to_string(),
        "--file".into(),
        "case.py".into(),
        "--operators".into(),
        "binary_add_sub".into(),
        "--jobs".into(),
        "1".into(),
        "--max-candidates".into(),
        limit.to_string(),
        "--min-free-space".into(),
        "1B".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python().display().to_string(),
        "-c".into(),
        "import case; assert case.first == 3 and case.second == 7".into(),
    ]
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One fixture traces the same candidates across three commands.
async fn literal_selector_fixture_respects_run_overflow_and_partial_verify() {
    let project = tempfile::tempdir().unwrap();
    let reports = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("case.py"), SOURCE).unwrap();
    for limit in [1, 2] {
        let plan_output = cli(&source_args("plan", project.path(), limit)).await;
        assert_eq!(
            plan_output.status.code(),
            Some(if limit == 1 { 4 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&plan_output.stderr)
        );
        let plan: Value = serde_json::from_slice(&plan_output.stdout).unwrap();
        assert_eq!(plan["truncated"], limit == 1);
        let candidates = plan["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), limit);
        for (index, (candidate, offset)) in candidates.iter().zip([10, 25]).enumerate() {
            assert_eq!(candidate["rank"], index + 1);
            assert_eq!(candidate["path"], "case.py");
            assert_eq!(candidate["span"], json!({"start":offset,"length":1}));
            assert_eq!(candidate["original"], "+");
            assert_eq!(candidate["replacement"], "-");
        }
        // Ranking annotations belong to the plan; process reports retain candidate identity.
        let projected_candidates: Vec<_> = candidates
            .iter()
            .map(|candidate| {
                let mut identity = candidate.as_object().unwrap().clone();
                for field in ["rank", "score", "ranking_reasons"] {
                    assert!(identity.remove(field).is_some());
                }
                Value::Object(identity)
            })
            .collect();
        let ids: Vec<_> = candidates.iter().map(|c| c["id"].clone()).collect();
        let run_output = cli(&source_args("run", project.path(), limit)).await;
        assert_eq!(
            run_output.status.code(),
            Some(if limit == 1 { 4 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&run_output.stderr)
        );
        let run: Value = serde_json::from_slice(&run_output.stdout).unwrap();
        let run_mutants = run["mutants"].as_array().unwrap();
        if limit == 1 {
            assert!(
                run_mutants.is_empty(),
                "ordinary overflow must not execute a retained prefix"
            );
            assert_eq!(run["summary"]["complete"], false);
        } else {
            assert_eq!(
                run_mutants
                    .iter()
                    .map(|m| m["candidate"]["id"].clone())
                    .collect::<Vec<_>>(),
                ids
            );
            assert_eq!(
                run_mutants
                    .iter()
                    .map(|m| &m["candidate"])
                    .collect::<Vec<_>>(),
                projected_candidates.iter().collect::<Vec<_>>()
            );
            assert!(run_mutants.iter().all(|m| m["status"] == "killed"));
            assert_eq!(run["summary"]["complete"], true);
        }
        let manifest = reports.path().join(format!("plan-{limit}.json"));
        std::fs::write(&manifest, &plan_output.stdout).unwrap();
        let verify_output = cli(&[
            "verify".into(),
            manifest.display().to_string(),
            "--top".into(),
            limit.to_string(),
        ])
        .await;
        assert_eq!(
            verify_output.status.code(),
            Some(if limit == 1 { 4 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&verify_output.stderr)
        );
        let verify: Value = serde_json::from_slice(&verify_output.stdout).unwrap();
        assert_eq!(
            verify["mutants"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["candidate"]["id"].clone())
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(verify["summary"]["complete"], limit == 2);
        assert_eq!(
            verify["mutants"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| &m["candidate"])
                .collect::<Vec<_>>(),
            projected_candidates.iter().collect::<Vec<_>>()
        );
        assert_eq!(plan["normalized_config"]["profile"], "full");
        assert!(plan["normalized_config"]["selection"].is_object());
        for field in ["profile", "selection"] {
            assert_eq!(
                verify["run"]["normalized_config"][field],
                plan["normalized_config"][field]
            );
        }
        assert_eq!(
            verify["run"]["normalized_config"]["limits"],
            plan["normalized_config"]["limits"]
        );
        assert_eq!(verify["run"]["verification_selection"]["selected"], limit);
        assert_eq!(
            std::fs::read_to_string(project.path().join("case.py")).unwrap(),
            SOURCE
        );
    }
}
