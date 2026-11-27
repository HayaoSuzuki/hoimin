use hoimin_cli::run_from;

#[tokio::test]
async fn reduce_help_is_available() {
    // Arrange
    let args = ["hoimin", "reduce", "--help"];
    // Act
    let code = run_from(args).await;
    // Assert
    assert_eq!(code, 0);
}

struct Project {
    root: tempfile::TempDir,
    artifacts: tempfile::TempDir,
    plan: std::path::PathBuf,
    candidate: String,
}
impl Project {
    async fn new(checks: &str) -> Self {
        let project = tempfile::tempdir().unwrap();
        let artifacts = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("subject.py"),
            "def value():\n    return 1\n",
        )
        .unwrap();
        std::fs::write(project.path().join("checks.py"), checks).unwrap();
        let plan = artifacts.path().join("plan.json");
        let python = std::env::var_os("PYTHON")
            .unwrap_or_else(|| if cfg!(windows) { "python" } else { "python3" }.into());
        let args = vec![
            "hoimin".into(), "plan".into(), "--root".into(), project.path().as_os_str().to_owned(),
            "--file".into(), "subject.py".into(), "--operators".into(), "integer_literal_neighbor".into(),
            "--allow-best-effort-memory".into(), "--min-free-space".into(), "1".into(),
            "--".into(), python, "-c".into(),
            "import runpy; ns=runpy.run_path('checks.py'); [v() for k,v in ns.items() if k.startswith('test_')]".into(),
        ];
        assert_eq!(
            hoimin_cli::run_with_writers(
                args,
                std::fs::File::create(&plan).unwrap(),
                std::io::stderr()
            )
            .await,
            0
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&plan).unwrap()).unwrap();
        let candidate = manifest["candidates"][0]["id"].as_str().unwrap().into();
        Self {
            root: project,
            artifacts,
            plan,
            candidate,
        }
    }
    async fn reduce(&self, selector: &str, percent: &str, budget: &str) -> i32 {
        let args = vec![
            "hoimin".into(),
            "reduce".into(),
            self.plan.as_os_str().to_owned(),
            "--candidate".into(),
            self.candidate.clone().into(),
            "--test".into(),
            selector.into(),
            "--total-timeout".into(),
            budget.into(),
            "--min-reduction".into(),
            percent.into(),
            "--output".into(),
            self.artifacts.path().join("reduced").into_os_string(),
        ];
        run_from(args).await
    }
}
const REDUNDANT: &str = "from subject import value\nimport time\ndef test_keep():\n    assert value() == 1\ndef test_slow():\n    time.sleep(0.08)\n    assert value() == 1\n";

#[tokio::test]
async fn redundant_definition_produces_verified_unapplied_diff() {
    // Arrange
    let project = Project::new(REDUNDANT).await;
    // Act
    let code = project.reduce("checks.py:test_slow", "5", "30s").await;
    // Assert
    assert_eq!(
        (
            code,
            std::fs::read(project.root.path().join("checks.py")).unwrap(),
            project
                .artifacts
                .path()
                .join("reduced/proposal.diff")
                .is_file()
        ),
        (0, REDUNDANT.as_bytes().to_vec(), true)
    );
}

#[rstest::rstest]
#[case("0")]
#[case("100")]
#[case("-1")]
#[case("NaN")]
#[tokio::test]
async fn invalid_percentage_is_input_error(#[case] percent: &str) {
    // Arrange
    let project = Project::new(REDUNDANT).await;
    // Act
    let code = project.reduce("checks.py:test_slow", percent, "10s").await;
    // Assert
    assert_eq!(code, 2);
}

#[tokio::test]
async fn loss_of_original_kill_produces_no_diff() {
    // Arrange
    let project =
        Project::new("from subject import value\ndef test_only():\n    assert value() == 1\n")
            .await;
    // Act
    let code = project.reduce("checks.py:test_only", "5", "20s").await;
    // Assert
    assert_eq!(
        (
            code,
            project
                .artifacts
                .path()
                .join("reduced/proposal.diff")
                .exists()
        ),
        (0, false)
    );
}

#[tokio::test]
async fn report_failure_preserves_a_diff_the_session_did_not_publish() {
    // Arrange
    // The test child creates the conflicts after reduce opens its fresh output directory.
    let project = Project::new(REDUNDANT).await;
    let output = project.artifacts.path().join("reduced");
    let checks = format!(
        "from pathlib import Path\noutput = Path({:?})\n(output / 'proposal.diff').write_bytes(b'independent diff')\n(output / 'report.json').write_bytes(b'independent report')\ndef test_fails():\n    assert False\n",
        output.to_str().unwrap(),
    );
    std::fs::write(project.root.path().join("checks.py"), checks).unwrap();
    // Act
    let code = project.reduce("checks.py:test_fails", "5", "20s").await;
    // Assert
    assert_eq!(code, 3);
    assert_eq!(
        std::fs::read(output.join("report.json")).unwrap(),
        b"independent report"
    );
    assert_eq!(
        std::fs::read(output.join("proposal.diff"))
            .expect("rollback must preserve an independently created diff"),
        b"independent diff"
    );
}

#[tokio::test]
async fn editing_mutation_source_is_input_error() {
    // Arrange
    let project = Project::new(REDUNDANT).await;
    // Act
    let code = project.reduce("subject.py:value", "5", "10s").await;
    // Assert
    assert_eq!(code, 2);
}

#[rstest::rstest]
#[case("subject.py")]
#[case("helper.txt")]
#[tokio::test]
async fn changed_original_snapshot_suppresses_diff(#[case] changed: &str) {
    // Arrange
    let project = Project::new(REDUNDANT).await;
    let changed_path = project.root.path().join(changed);
    let checks = format!(
        "from pathlib import Path\nPath({:?}).write_text('changed')\n{REDUNDANT}",
        changed_path.to_str().unwrap()
    );
    std::fs::write(project.root.path().join("helper.txt"), "original").unwrap();
    std::fs::write(project.root.path().join("checks.py"), checks).unwrap();
    // Act
    let code = project.reduce("checks.py:test_slow", "5", "30s").await;
    // Assert
    assert_eq!(
        (
            code,
            project
                .artifacts
                .path()
                .join("reduced/proposal.diff")
                .exists()
        ),
        (3, false)
    );
}

#[tokio::test]
async fn grouped_failure_is_split_without_combining_independent_deletions() {
    // Arrange
    let project=Project::new("from subject import value\nimport time\ndef test_a():\n    time.sleep(0.08)\n    assert value() == 1\ndef test_b():\n    time.sleep(0.08)\n    assert value() == 1\n").await;
    // Act
    let code = project.reduce("checks.py:test_*", "5", "30s").await;
    // Assert
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(project.artifacts.path().join("reduced/report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        (code, report["best"]["removed"].as_array().unwrap().len()),
        (0, 1)
    );
}

#[tokio::test]
async fn total_deadline_stops_original_execution_without_diff() {
    // Arrange
    let project = Project::new("import time\ndef test_wait():\n    time.sleep(5)\n").await;
    // Act
    let code = project.reduce("checks.py:test_wait", "5", "200ms").await;
    // Assert
    assert_eq!(
        (
            code,
            project
                .artifacts
                .path()
                .join("reduced/proposal.diff")
                .exists()
        ),
        (4, false)
    );
}

#[rstest::rstest]
#[case("def test_fail():\n    assert False\n", "checks.py:test_fail")]
#[case("def test_no_kill():\n    pass\n", "checks.py:test_no_kill")]
#[tokio::test]
async fn unusable_original_suite_never_starts_reduction(
    #[case] checks: &str,
    #[case] selector: &str,
) {
    // Arrange
    let project = Project::new(checks).await;
    // Act
    let code = project.reduce(selector, "5", "10s").await;
    // Assert
    assert_eq!(
        (
            code,
            project
                .artifacts
                .path()
                .join("reduced/proposal.diff")
                .exists()
        ),
        (2, false)
    );
}

#[tokio::test]
async fn insufficient_confirmation_reserve_is_reported_as_incomplete() {
    // Arrange
    let project = Project::new(REDUNDANT).await;
    // Act
    let code = project.reduce("checks.py:test_slow", "5", "700ms").await;
    // Assert
    assert_eq!(
        (
            code,
            project
                .artifacts
                .path()
                .join("reduced/proposal.diff")
                .exists()
        ),
        (4, false)
    );
}

#[tokio::test]
async fn reduction_preserves_workspace_execution_metadata() {
    // Arrange
    let project = Project::new(concat!(
        "from subject import value\n",
        "import os, time\n",
        "assert os.environ['HOIMIN_RUN_ID']\n",
        "assert os.path.samefile(os.environ['HOIMIN_WORKER_ROOT'], os.getcwd())\n",
        "def test_keep():\n",
        "    assert os.getenv('HOIMIN_MUTANT_ID') is None or value() == 1\n",
        "def test_slow():\n",
        "    time.sleep(0.08)\n",
        "    assert os.getenv('HOIMIN_MUTANT_ID') is None or value() == 1\n",
    ))
    .await;
    // Act
    let code = project.reduce("checks.py:test_slow", "5", "30s").await;
    // Assert
    assert_eq!(
        (
            code,
            project
                .artifacts
                .path()
                .join("reduced/proposal.diff")
                .exists()
        ),
        (0, true)
    );
}

fn mutant_timeouts(report: &serde_json::Value) -> std::collections::BTreeSet<u64> {
    report["observations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|observation| !observation["mutant"].is_null())
        .map(|observation| observation["timeout_ns"].as_u64().unwrap())
        .collect()
}

#[rstest::rstest]
#[case(hoimin_budget_math::MutantTimeout::Auto)]
#[case(hoimin_budget_math::MutantTimeout::Fixed(hoimin_budget_math::NonZeroDuration::new(std::time::Duration::from_millis(1234)).unwrap()))]
#[tokio::test]
async fn selected_timeout_is_frozen_for_every_mutant_execution(
    #[case] policy: hoimin_budget_math::MutantTimeout,
) {
    // Arrange
    let project = Project::new(REDUNDANT).await;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&project.plan).unwrap()).unwrap();
    manifest["normalized_config"]["limits"]["mutant_timeout"] =
        serde_json::to_value(policy).unwrap();
    std::fs::write(&project.plan, serde_json::to_vec(&manifest).unwrap()).unwrap();
    // Act
    let code = project.reduce("checks.py:test_slow", "5", "30s").await;
    // Assert
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(project.artifacts.path().join("reduced/report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(code, 0);
    assert_eq!(
        mutant_timeouts(&report),
        std::collections::BTreeSet::from([report["frozen_mutant_timeout_ns"].as_u64().unwrap()])
    );
}
