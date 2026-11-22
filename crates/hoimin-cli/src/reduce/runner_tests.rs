use super::*;
use crate::reduce::ReduceArgs;

async fn runner_fixture() -> (tempfile::TempDir, tempfile::TempDir, Runner) {
    let project = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("subject.py"), "value = 1\n").unwrap();
    std::fs::write(project.path().join("checks.py"), "def test_value(): pass\n").unwrap();
    let plan = output.path().join("plan.json");
    let args = vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        project.path().as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        "integer_literal_neighbor".into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1".into(),
        "--".into(),
        "unused-test-command".into(),
    ];
    assert_eq!(
        crate::run_with_writers(args, std::fs::File::create(&plan).unwrap(), std::io::sink()).await,
        0
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&plan).unwrap()).unwrap();
    let input = Input::prepare(&ReduceArgs {
        plan,
        candidate: vec![manifest["candidates"][0]["id"].as_str().unwrap().to_owned()],
        test: vec!["checks.py:test_value".into()],
        keep: vec![],
        total_timeout: "60s".into(),
        min_reduction: "5".into(),
        output: output.path().join("reduction"),
    })
    .await
    .unwrap();
    let artifacts = Artifacts::create(&input.output).unwrap();
    let start = Instant::now();
    let runner = Runner::new(&input, artifacts, start, start + Duration::from_secs(60))
        .await
        .unwrap();
    (project, output, runner)
}

#[rstest::rstest]
#[case(3)]
#[case(5)]
#[tokio::test]
async fn successful_search_keeps_remaining_budget_for_cleanup(#[case] elapsed: u64) {
    // Arrange: prepare real workspace capabilities without launching a test process.
    let (_project, _output, mut runner) = runner_fixture().await;
    runner.finish_execution(None).await.unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(elapsed)).await;

    // Act: cleanup starts after the failure grace, within the search's total budget.
    let result = runner.close().await;
    tokio::time::resume();

    // Assert: successful work must not fail merely because two seconds have elapsed.
    assert!(result.is_ok(), "{result:?}");
    assert!(!runner.root.path().exists());
}

#[rstest::rstest]
#[case::normal_completion(60, false, 62)]
#[case::failure_before_deadline(60, true, 2)]
#[case::at_deadline(0, true, 2)]
#[case::late_timeout(-1, true, 1)]
#[case::expired_grace(-3, true, 0)]
fn finalization_respects_the_original_absolute_deadline(
    #[case] remaining: i64,
    #[case] stopped: bool,
    #[case] expected: u64,
) {
    // Arrange
    let now = Instant::now();
    let duration = Duration::from_secs(remaining.unsigned_abs());
    let run_deadline = if remaining < 0 {
        now - duration
    } else {
        now + duration
    };
    // Act
    let deadline = finalization_deadline(run_deadline, stopped, now);
    // Assert
    assert_eq!(
        deadline.saturating_duration_since(now),
        Duration::from_secs(expected)
    );
}

proptest::proptest! {
    #[test]
    fn finalization_never_renews_expired_or_stopped_budgets(remaining in -10000i64..10000, stopped in proptest::bool::ANY) {
        // Arrange
        let now = Instant::now();
        let duration = Duration::from_millis(remaining.unsigned_abs());
        let run_deadline = if remaining < 0 { now - duration } else { now + duration };
        // Act
        let deadline = finalization_deadline(run_deadline, stopped, now);
        // Assert
        proptest::prop_assert!(deadline <= run_deadline + SHUTDOWN_GRACE);
        if stopped {
            proptest::prop_assert!(deadline <= now + SHUTDOWN_GRACE);
        } else {
            proptest::prop_assert!(deadline >= run_deadline);
        }
    }
}

#[tokio::test]
async fn stopped_search_keeps_only_its_shutdown_grace() {
    // Arrange
    let (_project, _output, mut runner) = runner_fixture().await;
    runner
        .finish_execution(Some(&Stop::budget()))
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3)).await;

    // Act
    let result = runner.close().await;
    tokio::time::resume();

    // Assert: exhausted cleanup is reported, with its owned root retained.
    assert!(result.is_err());
    assert!(runner.root.path().exists());
    // Release the synthetic retained root after checking the failed lifecycle.
    runner.root.cleanup(Duration::from_secs(2));
}

#[tokio::test]
async fn first_interrupt_during_cleanup_is_reported_within_the_grace() {
    // Arrange: hold the real coordinator lock so close must wait and retry.
    let (_project, _output, mut runner) = runner_fixture().await;
    runner.finish_execution(None).await.unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            runner
                .root
                .path()
                .parent()
                .unwrap()
                .join(".hoimin-coordinator"),
        )
        .unwrap();
    lock.lock().unwrap();
    let (signals, received) = tokio::sync::mpsc::unbounded_channel();
    runner.interrupts =
        crate::interrupt::spawn_test_monitor(received, |_| panic!("unexpected second interrupt"));
    tokio::time::pause();

    // Act: send the first signal while cleanup is already retrying contention.
    let (result, ()) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(3), runner.close()),
        async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            signals.send(Ok(())).unwrap();
        }
    );
    tokio::time::resume();
    drop(lock);
    runner.root.cleanup(Duration::from_secs(2));

    // Assert
    assert!(
        matches!(result, Ok(Err(Stop { exit_code: 130, .. }))),
        "{result:?}"
    );
}

#[tokio::test]
async fn interrupt_observed_during_drain_is_not_consumed_twice() {
    // Arrange
    let (_project, _output, mut runner) = runner_fixture().await;
    let (signals, received) = tokio::sync::mpsc::unbounded_channel();
    runner.interrupts =
        crate::interrupt::spawn_test_monitor(received, |_| panic!("unexpected second interrupt"));
    signals.send(Ok(())).unwrap();
    tokio::task::yield_now().await;

    // Act
    let stopped = runner.finish_execution(None).await;
    let cleanup = runner.close().await;

    // Assert: the first phase reports cancellation; cleanup remains safe to call.
    assert!(
        matches!(stopped, Err(Stop { exit_code: 130, .. })),
        "{stopped:?}"
    );
    assert!(cleanup.is_ok(), "{cleanup:?}");
    assert!(!runner.root.path().exists());
}
