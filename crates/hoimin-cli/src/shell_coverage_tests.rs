mod coverage_tests {
    use super::*;

    fn wait_for_guarded_dispatch(control: &RunControl) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while control.dispatched_effects.load(Ordering::Acquire) == 0 {
            assert!(
                Instant::now() < deadline,
                "run did not reach guarded dispatch"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    async fn wait_for_owned_task_finish(task: &tokio::task::AbortHandle) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !task.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("owned panic task must finish before draining joins");
    }

    #[test]
    fn default_run_control_starts_without_cancellation() {
        let control = RunControl::default();

        let actual = !control.request.is_cancelled();

        assert!(actual);
    }

    #[test]
    fn disk_meter_debug_does_not_expose_the_meter() {
        let meter = TestDiskMeter(Arc::new(std::sync::Mutex::new(Box::new(FailingDiskMeter))));

        assert_eq!(format!("{meter:?}"), "TestDiskMeter { .. }");
    }

    #[test]
    fn shared_diagnostic_capture_preserves_written_bytes() {
        let mut capture = SharedDiagnosticCapture::default();
        let written_bytes = capture.write(b"diagnostic").unwrap();
        capture.flush().unwrap();
        let captured_bytes = (*capture.0.lock().unwrap()).clone();

        assert_eq!(written_bytes, 10);
        assert_eq!(captured_bytes, b"diagnostic");
    }

    #[tokio::test]
    async fn blocking_preparation_reports_missing_targets() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let preflight = RunEffect::Preflight(Preflight { id: EffectId(501) });
        let error = prepare_blocking_effect(&mut context, preflight.clone())
            .err()
            .unwrap();

        let actual = (error.failure.code()).to_owned();

        assert_eq!(actual, "shell.targets.missing");
    }

    #[tokio::test]
    async fn preflight_preparation_reports_missing_workspace() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        context.resolved_targets = Some(Vec::new());
        context.workspace = None;
        let preflight = RunEffect::Preflight(Preflight { id: EffectId(501) });
        let error = prepare_blocking_effect(&mut context, preflight)
            .err()
            .unwrap();

        let actual = (error.failure.code()).to_owned();

        assert_eq!(actual, "shell.blocking_io");
    }

    #[tokio::test]
    async fn cleanup_preparation_reports_missing_workspace() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        context.workspace = None;
        let error = prepare_blocking_effect(
            &mut context,
            RunEffect::Cleanup(Cleanup {
                id: EffectId(502),
                reservations: Vec::new(),
            }),
        )
        .err()
        .unwrap();

        let actual = (error.failure.code()).to_owned();

        assert_eq!(actual, "shell.blocking_io");
    }

    #[tokio::test]
    async fn direct_candidate_read_reports_missing_replay() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        context.candidate_replay = None;
        let effect = candidate_read_effect(
            503,
            hoimin_core::CandidateSpoolRef {
                token: "missing".into(),
                records: 0,
            },
            hoimin_core::CandidateReadTarget::Next,
        );
        let event = execute_direct_io_effect(&mut context, effect);

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.blocking_io");

        assert!(actual);
    }

    #[tokio::test]
    async fn duplicate_candidate_replay_ownership_is_rejected() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let event = accept_blocking_completion(
            &mut context,
            BlockingEffectCompletion::Candidate {
                id: EffectId(504),
                replay: CandidateReplay::new([]),
                event: Box::new(RunEvent::CancellationRequested),
            },
        );

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.blocking_io");

        assert!(actual);
    }

    #[tokio::test]
    async fn duplicate_workspace_ownership_is_rejected_without_replacing_the_owner() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let (_other_project, _other_parent, mut other, _, _) = context_with_worker().await;
        let event = accept_blocking_completion(
            &mut context,
            BlockingEffectCompletion::OwnedWorkspace {
                id: EffectId(505),
                workspace: Box::new(other.workspace.take().unwrap()),
                event: Box::new(RunEvent::CancellationRequested),
                secondary_errors: Vec::new(),
                metrics_destination: None,
                session_path: None,
            },
        );

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.blocking_io");
        let actual_2 = context.workspace.is_some();

        assert!(actual);
        assert!(actual_2);
    }

    #[tokio::test]
    async fn session_open_reports_missing_path() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;

        let actual = (session(&mut context, EffectId(506))
            .await
            .err()
            .unwrap()
            .failure
            .code())
        .to_owned();

        assert_eq!(actual, "session.missing");
    }

    #[tokio::test]
    async fn session_open_reports_invalid_path() {
        let (project, _parent, mut context, _, _) = context_with_worker().await;
        context.session_path = Some(Utf8PathBuf::from_path_buf(project.path().to_owned()).unwrap());

        let actual = (session(&mut context, EffectId(507))
            .await
            .err()
            .unwrap()
            .failure
            .code())
        .to_owned();

        assert_eq!(actual, "session.open");
    }

    #[tokio::test]
    async fn unavailable_worker_environment_fails_before_running_a_baseline() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let request = RunProcess {
            id: EffectId(508),
            context: hoimin_core::BaselineContext {
                worker: 99,
                run_id: "run".into(),
            },
            argv: Vec::new(),
            cwd: ".".into(),
            limits: hoimin_core::ProcessLimits {
                timeout: Duration::from_secs(1),
                max_output_bytes: 1,
                max_memory_bytes: 1,
                max_processes: 1,
            },
        };
        let event = execute_effect(&mut context, RunEffect::RunBaseline(request)).await;

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.worker.environment");

        assert!(actual);
    }

    #[test]
    fn session_artifact_preflight_reports_invalid_roots() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut data = config.into_data();
        data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
            path: Utf8PathBuf::from_path_buf(project.path().join("session.sqlite3")).unwrap(),
        });
        data.selection.root =
            Utf8PathBuf::from_path_buf(project.path().join("missing-root")).unwrap();
        let config = RunConfig::try_from(data).unwrap();

        let actual = prepare_session_artifacts(&config).is_err();

        assert!(actual);
    }

    #[tokio::test]
    async fn scheduler_owned_budget_effect_rejects_direct_execution() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let event = execute_effect_with_cancellation(
            &mut context,
            RunEffect::ObserveRemainingBudget(ObserveRemainingBudget { id: EffectId(509) }),
            ProcessCancellation::new(),
        )
        .await;

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.budget.scheduler");

        assert!(actual);
    }

    #[tokio::test]
    async fn unavailable_worker_environment_fails_before_running_a_mutant() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let request = RunProcess {
            id: EffectId(510),
            context: hoimin_core::MutantContext {
                worker: 99,
                run_id: "run".into(),
                mutant_id: "mutant".into(),
            },
            argv: Vec::new(),
            cwd: ".".into(),
            limits: hoimin_core::ProcessLimits {
                timeout: Duration::from_secs(1),
                max_output_bytes: 1,
                max_memory_bytes: 1,
                max_processes: 1,
            },
        };
        let event = execute_effect(&mut context, RunEffect::RunMutant(request)).await;

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.worker.environment");

        assert!(actual);
    }

    #[tokio::test]
    async fn resource_cleanup_reports_missing_ownership() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        context.workspace = None;
        let missing = close_context_resources(
            &mut context,
            &budget,
            false,
            true,
            // Missing ownership must reject cleanup before this failure sentinel can run.
            Box::new(
                #[cfg_attr(coverage_nightly, coverage(off))]
                || panic!("missing ownership reached the cleanup hook"),
            ),
        )
        .await;

        let actual = missing.workspace.unwrap_err();

        assert_eq!(
            actual,
            "workspace ownership is unavailable during final cleanup"
        );
    }

    #[tokio::test]
    async fn resource_cleanup_reports_a_panicking_start_hook() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        let failed = close_context_resources(
            &mut context,
            &budget,
            false,
            true,
            Box::new(|| panic!("cleanup hook panicked")),
        )
        .await;

        let actual = failed
            .workspace
            .unwrap_err()
            .contains("cleanup hook panicked");
        let actual_2 = failed.process.is_ok();
        let actual_3 = failed.expiry.is_none();

        assert!(actual);
        assert!(actual_2);
        assert!(actual_3);
    }

    #[tokio::test]
    async fn metrics_finalization_preserves_warnings_when_the_owned_task_panics() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        let output = crate::metrics::MetricsOutput::open(&path).unwrap();
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        let finalized = finalize_metrics_with_shutdown(
            output,
            None,
            None,
            0,
            0,
            vec![("previous", "preserve".into())],
            &budget,
            Box::new(|| panic!("metrics hook panicked")),
        )
        .await;

        let actual = finalized.warnings[1].1.contains("metrics hook panicked");
        let actual_2 = finalized.expiry.is_none();
        let actual_3 = !path.exists();

        assert_eq!(finalized.warnings[0], ("previous", "preserve".into()));
        assert_eq!(finalized.warnings[1].0, "metrics.write");
        assert!(actual);
        assert!(actual_2);
        assert!(actual_3);
    }

    #[test]
    fn finalization_errors_have_a_fallback_code_and_deduplicate_secondary_failures() {
        let mut summary = hoimin_core::RunSummary {
            resolved_mutant_timeout: None,
            schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
            sequence: 1,
            run_id: "run".into(),
            counts: hoimin_core::MutationSummary::default(),
            complete: true,
            exit_code: 0,
            disk: hoimin_core::DiskRunSummary::unmeasured(8, 10),
            verification_selection: None,
        };
        apply_finalization_errors_to_summary(&mut summary, &["unstructured failure".into()]);
        let stop = summary.disk.stop.as_ref().unwrap();
        let primary_failure_code = (stop.code).clone();
        let exit_code_after_first_failure = summary.exit_code;
        apply_finalization_errors_to_summary(
            &mut summary,
            &[
                "unstructured failure".into(),
                "cleanup.failed: detail".into(),
                "cleanup.failed: detail".into(),
            ],
        );
        let secondary_failure_count = summary.disk.stop.as_ref().unwrap().secondary.len();
        let incomplete = !summary.complete;

        assert_eq!(primary_failure_code, "run.finalization.failed");
        assert_eq!(exit_code_after_first_failure, 2);
        assert_eq!(secondary_failure_count, 1);
        assert!(incomplete);
    }

    #[cfg(target_os = "linux")]
    #[rstest::rstest]
    #[case::artifact(0, "artifact path")]
    #[case::database(1, "database path")]
    fn session_artifacts_reject_unrepresentable_native_names(
        #[case] root_index: usize,
        #[case] expected: &str,
    ) {
        use std::os::unix::{ffi::OsStringExt, fs::symlink};
        let project = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let root = [project.path(), external.path()][root_index];
        let native = root.join(OsString::from_vec(b"native-\xff".to_vec()));
        std::fs::create_dir(&native).unwrap();
        let link = project.path().join("alias");
        symlink(&native, &link).unwrap();
        let mut data = config.clone().into_data();
        data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
            path: Utf8PathBuf::from_path_buf(link.join("session.sqlite3")).unwrap(),
        });
        let native_path_rejected = prepare_session_artifacts(&RunConfig::try_from(data).unwrap())
            .unwrap_err()
            .contains(expected);
        std::fs::remove_file(link).unwrap();

        assert!(native_path_rejected);
    }

    #[cfg(unix)]
    #[test]
    fn session_artifacts_reject_a_symlink_loop_in_the_ownership_tree() {
        use std::os::unix::fs::symlink;
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let lock = project.path().join(".session.sqlite3.hoimin-locks");
        symlink(&lock, &lock).unwrap();
        let mut data = config.into_data();
        data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
            path: Utf8PathBuf::from_path_buf(project.path().join("session.sqlite3")).unwrap(),
        });

        let actual = prepare_session_artifacts(&RunConfig::try_from(data).unwrap()).is_err();

        assert!(actual);
    }

    #[cfg(unix)]
    #[test]
    fn session_artifact_names_reject_literal_backslashes() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut data = config.into_data();
        data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
            path: Utf8PathBuf::from_path_buf(project.path().join("bad\\session.sqlite3")).unwrap(),
        });

        let actual = prepare_session_artifacts(&RunConfig::try_from(data).unwrap())
            .unwrap_err()
            .contains("literal backslash");

        assert!(actual);
    }
    #[rstest::rstest]
    #[case::load(|_: MutationCandidate| RunEffect::LoadSession(hoimin_core::LoadSession {
                id: EffectId(511),
                fingerprint: hoimin_core::RunFingerprint::from_bytes([1; 32]),
                max_mutants: std::num::NonZeroUsize::new(1).unwrap(),
            }))]
    #[case::lookup(|_: MutationCandidate| RunEffect::LookupStoredResult(hoimin_core::LookupStoredResult {
                id: EffectId(512),
                worker: 0,
                run_id: "run".into(),
                mutant_id: "mutant".into(),
            }))]
    #[case::begin(|_: MutationCandidate| RunEffect::BeginSession(hoimin_core::BeginSession {
                id: EffectId(513),
                fingerprint: hoimin_core::RunFingerprint::from_bytes([1; 32]),
                max_mutants: std::num::NonZeroUsize::new(1).unwrap(),
                run_id: "run".into(),
            }))]
    #[case::persist(|candidate: MutationCandidate| RunEffect::PersistResult(hoimin_core::PersistResult {
                id: EffectId(514),
                worker: 0,
                result: hoimin_core::MutantResult {
                    run_id: "run".into(),
                    candidate,
                    outcome: hoimin_core::MutationOutcome::NotRun,
                    resource_mode: ResourceMode::BestEffort,
                    diagnostics: Vec::new(),
                },
            }))]
    #[case::finish(|_: MutationCandidate| RunEffect::FinishSession(hoimin_core::FinishSession {
                id: EffectId(515),
                run_id: "run".into(),
                complete: true,
            }))]
    #[tokio::test]
    async fn session_effects_without_configuration_report_missing_owner(
        #[case] effect: fn(MutationCandidate) -> RunEffect,
    ) {
        let (_project, _parent, mut context, candidate, _) = context_with_worker().await;
        let effect = effect(candidate);
        let expected = effect.id();
        let event = execute_effect(&mut context, effect).await;

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.id == expected && error.failure.code() == "session.missing");

        assert!(actual);
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_that_wins_the_dispatch_gate_prevents_baseline_spawn() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project);
        let control = RunControl::new();
        let observing = control.clone();
        let gate = control.request.begin_spawn();
        let running_control = control.clone();
        let running = tokio::spawn(async move {
            run_loop_with_control(config, Vec::new(), Vec::new(), running_control).await
        });
        tokio::task::block_in_place(|| {
            wait_for_guarded_dispatch(&observing);
            observing
                .cancelled_at
                .lock()
                .unwrap()
                .get_or_insert_with(tokio::time::Instant::now);
            observing.request.cancel_under_held_spawn_gate_for_tests();
            drop(gate);
        });

        let actual = running.await.unwrap().unwrap();
        let actual_2 = observing.max_process_tasks();

        assert_eq!(actual, 130);
        assert_eq!(actual_2, 0);
    }
    #[tokio::test]
    async fn direct_workspace_adapter_restores_and_verifies_the_original_after_replay() {
        let (_project, _parent, mut context, candidate, retry) = context_with_worker().await;
        let id = retry.id();
        let root = context.workspace().worker(0).unwrap().root().to_owned();
        let duplicate = execute_direct_io_effect(&mut context, RunEffect::CreateWorker(retry));
        let duplicate_rejected =
            matches!(duplicate, RunEvent::EffectFailed(error) if error.id == id);
        let worker_root_before_replay = context.workspace().worker(0).unwrap().root().to_owned();
        let spool_directory = tempfile::tempdir().unwrap();
        let mut store = crate::analyzer::CandidateStore::new_in(1, spool_directory.path()).unwrap();
        store.push(&candidate).unwrap();
        let spool = store.finish().unwrap();
        context.candidate_replay = Some(CandidateReplay::new([candidate.id.clone()]));
        let loaded = execute_direct_io_effect(
            &mut context,
            candidate_read_effect(520, spool, hoimin_core::CandidateReadTarget::Next),
        );
        let candidate_loaded = matches!(loaded, RunEvent::CandidateLoaded(CandidateLoaded { candidate: Some(value), .. }) if value == candidate);
        let mutated = execute_direct_io_effect(
            &mut context,
            RunEffect::ApplyMutation(ApplyMutation {
                id: EffectId(521),
                worker: 0,
                candidate,
            }),
        );
        let mutation_applied = matches!(mutated, RunEvent::MutationApplied(_));
        let reset = execute_direct_io_effect(
            &mut context,
            RunEffect::ResetWorker(ResetWorker {
                id: EffectId(522),
                worker: 0,
            }),
        );
        let worker_reset = matches!(reset, RunEvent::WorkerReset(_));
        let verified = execute_direct_io_effect(
            &mut context,
            RunEffect::VerifyOriginals(VerifyOriginals {
                id: EffectId(523),
                checkpoint: IntegrityCheckpoint::PreFinalReport,
            }),
        );
        let original_verified = matches!(verified, RunEvent::OriginalsVerified(_));
        let restored_source = context
            .workspace()
            .worker(0)
            .unwrap()
            .read("pkg/a.py")
            .unwrap();

        assert!(duplicate_rejected);
        assert_eq!(worker_root_before_replay, root);
        assert!(candidate_loaded);
        assert!(mutation_applied);
        assert!(worker_reset);
        assert!(original_verified);
        assert_eq!(restored_source, b"original\n");
    }

    #[tokio::test(start_paused = true)]
    async fn live_process_fixture_reports_completion_before_readiness() {
        let mut completed = std::future::ready(17);

        let error = wait_for_live_process(&mut completed, || false)
            .await
            .unwrap_err();

        assert!(error.contains("finished before fixture readiness"));
    }

    #[tokio::test(start_paused = true)]
    async fn live_process_fixture_bounds_an_unresponsive_readiness_wait() {
        let mut unresponsive = std::future::pending::<()>();

        let error = wait_for_live_process(&mut unresponsive, || false)
            .await
            .unwrap_err();

        assert_eq!(error, "live process fixture did not become ready");
    }

    #[tokio::test(start_paused = true)]
    async fn disk_stop_fixture_cancels_an_unresponsive_run_at_its_deadline() {
        let mut unresponsive = std::future::pending::<()>();
        let control = RunControl::new();

        let error = wait_for_disk_stop(&mut unresponsive, &control)
            .await
            .unwrap_err();

        assert_eq!(
            error,
            "periodic disk stop did not interrupt the active process"
        );
        assert!(control.is_cancelled());
    }

    #[test]
    fn startup_events_do_not_establish_a_shutdown_budget() {
        let now = tokio::time::Instant::now();
        let mut budget = None;

        let actual = establish_event_shutdown_budget(
            &mut budget,
            &RunEvent::StartRequested(StartRequested),
            now + Duration::from_secs(1),
            now,
            Duration::from_secs(1),
        );

        assert_eq!(actual, None);
        assert_eq!(budget, None);
    }
    #[tokio::test]
    async fn shutdown_drain_keeps_the_first_outer_io_task_failure_and_clears_accounting() {
        let mut io_tasks = JoinSet::new();
        let first = io_tasks.spawn(async { panic!("first outer I/O panic") });
        wait_for_owned_task_finish(&first).await;
        let second = io_tasks.spawn(async { panic!("second outer I/O panic") });
        wait_for_owned_task_finish(&second).await;
        let (sender, mut receiver) = mpsc::channel(1);
        drop(sender);
        let mut process_tasks = JoinSet::new();
        let mut in_flight = 2;
        let mut expiry_reported = false;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(1),
        );
        let failure = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await
        .unwrap_err();

        let actual = failure.contains("blocking I/O task failed while stopping");
        let actual_2 = failure.contains("first outer I/O panic");
        let actual_3 = !failure.contains("second outer I/O panic");
        let actual_4 = io_tasks.is_empty();

        assert!(actual);
        assert!(actual_2);
        assert!(actual_3);
        assert_eq!(in_flight, 0);
        assert!(actual_4);
        assert!(!expiry_reported);
        assert_eq!(warnings, Vec::<(&str, String)>::new());
    }
    #[derive(Clone, Copy)]
    enum ReleasePolicy {
        AfterObserved(&'static str),
        AfterRun,
    }

    enum ReportTrigger {
        Signal(Result<(), String>),
        Cancellation,
    }

    pub(super) async fn wait_for_finalization_event(control: &RunControl, event: &'static str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::task::yield_now().await;
                if control.finalization_events().contains(&event) {
                    break;
                }
            }
        })
        .await
        .expect("owned run must publish the awaited finalization event");
    }

    async fn release_after_observation(
        policy: ReleasePolicy,
        control: &RunControl,
        release: &mut Option<std::sync::mpsc::Sender<()>>,
    ) {
        match policy {
            ReleasePolicy::AfterObserved(event) => {
                wait_for_finalization_event(control, event).await;
                release.take().unwrap().send(()).unwrap();
            }
            ReleasePolicy::AfterRun => {}
        }
    }

    fn release_remaining_owner(release: Option<std::sync::mpsc::Sender<()>>) {
        if let Some(release) = release {
            release.send(()).unwrap();
        }
    }

    struct ReportObservation {
        result: Result<i32, String>,
        retained_delivery: bool,
        acknowledged: bool,
    }

    async fn observe_paused_report(
        marker: &'static [u8],
        pause_flush: bool,
        grace: Duration,
        trigger: ReportTrigger,
        release_policy: ReleasePolicy,
    ) -> ReportObservation {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut control = RunControl::new();
        control.shutdown_grace = grace;
        let observed = control.clone();
        let cancelling = control.clone();
        let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        let interrupts = crate::interrupt::spawn_test_monitor(signal_rx, drop);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = PausedReportWriter {
            entered: Some(entered_tx),
            release: release_rx,
            pause_flush,
            marker,
            record: Vec::new(),
            target_seen: false,
        };
        let mut run = Box::pin(run_loop_prepared_timed(
            config,
            writer,
            Vec::new(),
            control,
            hoimin_core::RunSelection::all(),
            None,
            timing,
            interrupts,
        ));
        let mut release = Some(release_tx);
        let observation = async {
            entered_rx.await.unwrap();
            match trigger {
                ReportTrigger::Signal(signal) => signal_tx.send(signal).unwrap(),
                ReportTrigger::Cancellation => cancelling.cancel(),
            }
            release_after_observation(release_policy, &observed, &mut release).await;
        };
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(&mut run, observation)
        })
        .await;
        let roots = observed.managed_root_paths();
        let retained_delivery = roots[1].exists();
        let acknowledged = observed
            .finalization_events()
            .contains(&"output_acknowledged");
        release_remaining_owner(release);
        drop(run);
        let (result, ()) = completed.unwrap();
        ReportObservation {
            result,
            retained_delivery,
            acknowledged,
        }
    }

    #[rstest::rstest]
    #[case::write(false)]
    #[case::flush(true)]
    #[tokio::test(flavor = "current_thread")]
    async fn final_report_interrupt_retains_unacknowledged_delivery(#[case] pause_flush: bool) {
        let observation = observe_paused_report(
            b"run_finished",
            pause_flush,
            Duration::from_millis(100),
            ReportTrigger::Signal(Ok(())),
            ReleasePolicy::AfterRun,
        )
        .await;

        let actual = observation.result.unwrap_or(2);

        assert_eq!(actual, 2);
        assert!(observation.retained_delivery);
        assert!(!observation.acknowledged);
    }

    #[rstest::rstest]
    #[case::write(false)]
    #[case::flush(true)]
    #[tokio::test(flavor = "current_thread")]
    async fn final_report_handler_failure_retains_unacknowledged_delivery(
        #[case] pause_flush: bool,
    ) {
        let observation = observe_paused_report(
            b"run_finished",
            pause_flush,
            Duration::from_millis(100),
            ReportTrigger::Signal(Err("install Ctrl+C handler: injected failure".into())),
            ReleasePolicy::AfterRun,
        )
        .await;

        let actual = observation
            .result
            .unwrap_err()
            .contains("install Ctrl+C handler: injected failure");

        assert!(actual);
        assert!(observation.retained_delivery);
        assert!(!observation.acknowledged);
    }

    #[rstest::rstest]
    #[case::write(false)]
    #[case::flush(true)]
    #[tokio::test(flavor = "current_thread")]
    async fn serial_report_interrupt_preserves_acknowledged_cancellation(
        #[case] pause_flush: bool,
    ) {
        let observation = observe_paused_report(
            b"baseline_finished",
            pause_flush,
            Duration::from_secs(2),
            ReportTrigger::Signal(Ok(())),
            ReleasePolicy::AfterObserved("interrupt_observed"),
        )
        .await;

        let actual = observation.result.unwrap();

        assert_eq!(actual, 130);
        assert!(observation.acknowledged);
    }

    #[rstest::rstest]
    #[case::write(false)]
    #[case::flush(true)]
    #[tokio::test(flavor = "current_thread")]
    async fn serial_report_handler_failure_remains_unacknowledged(#[case] pause_flush: bool) {
        let observation = observe_paused_report(
            b"baseline_finished",
            pause_flush,
            Duration::from_secs(2),
            ReportTrigger::Signal(Err("install Ctrl+C handler: injected failure".into())),
            ReleasePolicy::AfterObserved("interrupt_observed"),
        )
        .await;

        let actual = observation
            .result
            .unwrap_err()
            .contains("install Ctrl+C handler: injected failure");

        assert!(actual);
        assert!(!observation.acknowledged);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn serial_cancellation_acknowledges_delivery_finished_inside_grace() {
        let observation = observe_paused_report(
            b"baseline_finished",
            false,
            Duration::from_secs(2),
            ReportTrigger::Cancellation,
            ReleasePolicy::AfterObserved("serial_stop_observed"),
        )
        .await;

        let actual = observation.result.unwrap();

        assert_eq!(actual, 130);
        assert!(observation.acknowledged);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn serial_cancellation_keeps_expired_delivery_unacknowledged() {
        let observation = observe_paused_report(
            b"baseline_finished",
            false,
            Duration::from_millis(100),
            ReportTrigger::Cancellation,
            ReleasePolicy::AfterRun,
        )
        .await;

        let actual = observation
            .result
            .unwrap_err()
            .contains("cancellation: shutdown grace expired");

        assert!(actual);
        assert!(!observation.acknowledged);
    }

    async fn observe_interrupted_materialization(
        signal: Result<(), String>,
        grace: Duration,
        release_policy: ReleasePolicy,
    ) -> (Result<i32, String>, usize) {
        let project = tempfile::tempdir().unwrap();
        let metrics = tempfile::tempdir().unwrap();
        let mut data = paused_materialization_config(&project, "5s").into_data();
        data.output.metrics =
            Some(Utf8PathBuf::from_path_buf(metrics.path().join("metrics.json")).unwrap());
        let config = RunConfig::try_from(data)
            .unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let (control, mut pause) =
            RunControl::with_materialization_pause_and_shutdown_grace(0, grace);
        let observed = control.clone();
        let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        let interrupts = crate::interrupt::spawn_test_monitor(signal_rx, drop);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let controller = tokio::task::spawn_blocking(move || {
            pause.wait_until_entered(Duration::from_secs(5)).unwrap();
            let guard = pause.release_guard();
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            drop(guard);
        });
        let mut run = Box::pin(run_loop_prepared_timed(
            config,
            Vec::new(),
            Vec::new(),
            control,
            hoimin_core::RunSelection::all(),
            None,
            timing,
            interrupts,
        ));
        let mut release = Some(release_tx);
        let observation = async {
            entered_rx.await.unwrap();
            signal_tx.send(signal).unwrap();
            release_after_observation(release_policy, &observed, &mut release).await;
        };
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(&mut run, observation)
        })
        .await;
        release_remaining_owner(release);
        drop(run);
        controller.await.unwrap();
        let (result, ()) = completed.unwrap();
        (result, observed.max_process_tasks())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn materialization_interrupt_with_cooperative_io_preserves_cancellation() {
        let (result, processes) = observe_interrupted_materialization(
            Ok(()),
            Duration::from_secs(2),
            ReleasePolicy::AfterObserved("interrupt_observed"),
        )
        .await;

        let actual = result.unwrap();

        assert_eq!(actual, 130);
        assert_eq!(processes, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn materialization_handler_failure_with_cooperative_io_preserves_primary() {
        let (result, processes) = observe_interrupted_materialization(
            Err("install Ctrl+C handler: materialization failure".into()),
            Duration::from_secs(2),
            ReleasePolicy::AfterObserved("interrupt_observed"),
        )
        .await;
        let error = result.unwrap_err();

        let actual = error.starts_with("install Ctrl+C handler: materialization failure");
        let actual_2 = !error.contains("shutdown grace expired");

        assert!(actual, "{error}");
        assert!(actual_2, "{error}");
        assert_eq!(processes, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn materialization_interrupt_with_stalled_io_preserves_drain_evidence() {
        let (result, processes) = observe_interrupted_materialization(
            Ok(()),
            Duration::from_millis(100),
            ReleasePolicy::AfterRun,
        )
        .await;
        let error = result.unwrap_err();

        let actual = error.contains("cancellation: shutdown grace expired");
        let actual_2 = error.contains("blocking I/O tasks: 1");

        assert!(actual, "{error}");
        assert!(actual_2, "{error}");
        assert_eq!(processes, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn materialization_handler_failure_with_stalled_io_preserves_drain_evidence() {
        let (result, processes) = observe_interrupted_materialization(
            Err("install Ctrl+C handler: materialization failure".into()),
            Duration::from_millis(100),
            ReleasePolicy::AfterRun,
        )
        .await;
        let error = result.unwrap_err();

        let actual = error.starts_with("install Ctrl+C handler: materialization failure");
        let actual_2 = error.contains("shutdown grace expired");

        assert!(actual, "{error}");
        assert!(actual_2, "{error}");
        assert_eq!(processes, 0);
    }

    fn inject_process_join_panic(control: &RunControl) {
        control
            .process_completion_panic
            .store(true, Ordering::Release);
    }
    fn inject_io_join_panic(control: &RunControl) {
        control.io_completion_panic.store(true, Ordering::Release);
    }

    #[rstest::rstest]
    #[case::process_owner(|control: &RunControl| inject_process_join_panic(control), "process task failed")]
    #[case::blocking_io_owner(|control: &RunControl| inject_io_join_panic(control), "blocking I/O task failed")]
    #[tokio::test(flavor = "current_thread")]
    async fn delivered_completions_do_not_hide_failed_outer_task_joins(
        #[case] inject: fn(&RunControl),
        #[case] task_kind: &str,
    ) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let control = RunControl::new();
        inject(&control);
        let observed = control.clone();
        let error = run_loop_prepared(
            config,
            Vec::new(),
            Vec::new(),
            control,
            hoimin_core::RunSelection::all(),
            None,
        )
        .await
        .unwrap_err();

        let actual = error.contains(task_kind);
        let actual_2 = error.contains("post-delivery panic");
        let actual_3 = !error.contains("shutdown grace expired");
        let actual_4 = !observed
            .finalization_events()
            .contains(&"output_acknowledged");

        assert!(actual, "{error}");
        assert!(actual_2, "{error}");
        assert!(actual_3, "{error}");
        assert!(actual_4);
    }

    struct MetricsObservation {
        _output: TempDir,
        path: std::path::PathBuf,
        result: Result<i32, String>,
        diagnostics: String,
    }

    async fn observe_outer_metrics(relative: &str, process_failure: bool) -> MetricsObservation {
        let project = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let path = output.path().join(relative);
        let mut data = output_failure_test_config(&project).into_data();
        data.output.metrics = Some(Utf8PathBuf::from_path_buf(path.clone()).unwrap());
        let config = RunConfig::try_from(data)
            .unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let control = RunControl::new();
        control
            .process_completion_panic
            .store(process_failure, Ordering::Release);
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let result = run_loop_prepared(
            config,
            stdout.clone(),
            stderr.clone(),
            control,
            hoimin_core::RunSelection::all(),
            None,
        )
        .await;
        let mut diagnostics = stdout.snapshot();
        diagnostics.extend(stderr.snapshot());
        MetricsObservation {
            _output: output,
            path,
            result,
            diagnostics: String::from_utf8(diagnostics).unwrap(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn outer_metrics_success_writes_a_valid_metrics_object() {
        let observation = observe_outer_metrics("metrics.json", false).await;
        let exit_code = observation.result.unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(observation.path).unwrap()).unwrap();
        let metrics_object = report.is_object();
        let no_incomplete_warning = !observation.diagnostics.contains("metrics.incomplete");

        assert_eq!(exit_code, 0);
        assert!(metrics_object);
        assert!(no_incomplete_warning);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn outer_metrics_unavailable_destination_reports_unwritten_output() {
        let observation = observe_outer_metrics("missing/metrics.json", false).await;

        let actual = observation.result.unwrap();
        let actual_2 = !observation.path.exists();
        let actual_3 = observation.diagnostics.contains("metrics.write");

        assert_eq!(actual, 0);
        assert!(actual_2);
        assert!(actual_3, "{}", observation.diagnostics);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn outer_metrics_failed_owned_task_reports_incomplete_output() {
        let observation = observe_outer_metrics("metrics.json", true).await;

        let actual = observation
            .result
            .unwrap_err()
            .contains("injected process post-delivery panic");
        let actual_2 = !observation.path.exists();
        let actual_3 = observation.diagnostics.contains("metrics.incomplete");

        assert!(actual);
        assert!(actual_2);
        assert!(actual_3, "{}", observation.diagnostics);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_mutant_run_persists_its_result_and_finishes_metrics_before_releasing_roots() {
        let project = tempfile::tempdir().unwrap();
        let artifacts = tempfile::tempdir().unwrap();
        let mut data = output_failure_test_config(&project).into_data();
        std::fs::write(
            project.path().join("target.py"),
            "def compute():\n    return 1 + 2\n",
        )
        .unwrap();
        data.limits.max_mutants = std::num::NonZeroUsize::new(100).unwrap();
        let metrics = artifacts.path().join("metrics.json");
        let session = artifacts.path().join("session.sqlite3");
        data.output.metrics = Some(Utf8PathBuf::from_path_buf(metrics.clone()).unwrap());
        data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
            path: Utf8PathBuf::from_path_buf(session.clone()).unwrap(),
        });
        let config = RunConfig::try_from(data)
            .unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let control = RunControl::new();
        let observed = control.clone();
        let stdout = CollectingWriter::default();
        let result = run_loop_prepared(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            hoimin_core::RunSelection::all(),
            None,
        )
        .await
        .unwrap();
        let exit_code = result;
        let report = String::from_utf8(stdout.snapshot()).unwrap();
        let mutant_result_emitted = report.contains("mutant_finished");
        let connection = rusqlite::Connection::open(session).unwrap();
        let result_persisted = connection
            .query_row("SELECT count(*) FROM results", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
            > 0;
        let run_complete = connection
            .query_row("SELECT complete FROM runs", [], |row| row.get::<_, i64>(0))
            .unwrap();
        let metrics: serde_json::Value =
            serde_json::from_slice(&std::fs::read(metrics).unwrap()).unwrap();
        let metrics_object = metrics.is_object();
        let output_acknowledged = observed
            .finalization_events()
            .contains(&"output_acknowledged");
        let roots_removed = observed
            .managed_root_paths()
            .iter()
            .all(|path| !path.exists());

        assert_eq!(exit_code, 0);
        assert!(mutant_result_emitted, "{report}");
        assert!(result_persisted);
        assert_eq!(run_complete, 1);
        assert!(metrics_object);
        assert!(output_acknowledged);
        assert!(roots_removed);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn saved_plan_execution_preserves_selected_candidate_membership() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        std::fs::write(
            project.path().join("target.py"),
            "def compute():\n    return 1 + 2\n",
        )
        .unwrap();
        let output = crate::plan::create(
            config,
            hoimin_core::DiscoveryConfig::new(Duration::from_secs(10)).unwrap(),
        )
        .await
        .unwrap();
        let selected = output.manifest.candidates[0].id.clone();
        let artifact = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            artifact.path(),
            serde_json::to_vec(&output.manifest).unwrap(),
        )
        .unwrap();
        let verified =
            crate::plan::prepare_verify(artifact.path(), std::slice::from_ref(&selected))
                .await
                .unwrap();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let exit_code = verified
            .execute(stdout.clone(), stderr.clone())
            .await
            .unwrap();
        let events: Vec<OutputEvent> = String::from_utf8(stdout.snapshot())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let finished: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                OutputEvent::MutantFinished(value) => Some(value),
                _ => None,
            })
            .collect();
        let finished_candidate_count = finished.len();
        let event = serde_json::to_string(finished[0]).unwrap();
        let selected_candidate_reported = event.contains(&selected);
        let diagnostics = stderr.snapshot();

        assert_eq!(exit_code, 0);
        assert_eq!(finished_candidate_count, 1);
        assert!(selected_candidate_reported, "{event}");
        assert_eq!(diagnostics, Vec::<u8>::new());
    }

    async fn baseline_spool_diagnostics(
        token: &str,
        prepare: fn(camino::Utf8PathBuf) -> std::io::Result<()>,
    ) -> Vec<serde_json::Value> {
        let directory = tempfile::tempdir().unwrap();
        let backend = crate::resource::ResourceBackend::Portable(
            crate::resource::PortableBackend::new(true).unwrap(),
        );
        let process = ProcessHandler::new(
            backend,
            Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap(),
        );
        let mut baseline = hoimin_core::BaselineFinished {
            schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
            sequence: 7,
            run_id: "spool-read-failure".into(),
            termination: hoimin_core::ProcessTermination::Exit(1),
            elapsed_ms: 1,
            resource_mode: process.mode(),
            output: hoimin_core::OutputSpoolRef {
                token: "00000000-0000-0000-0000-000000000001".into(),
                retained: 4,
                observed: 4,
            },
        };
        prepare(process.spool_path(&baseline.output).unwrap()).unwrap();
        baseline.output.token = token.into();
        let capture = SharedDiagnosticCapture::default();
        let mut report = ReportDelivery::new(
            ReportHandler::new(Vec::new(), capture.clone()),
            Arc::new(()),
        );
        crate::baseline_output::emit(&process, &mut report, &baseline).await;
        report.flush().await.unwrap();
        let bytes = capture.0.lock().unwrap();
        std::str::from_utf8(&bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[rstest::rstest]
    #[case::invalid_token("../outside", |_| Ok(()))]
    #[case::missing("00000000-0000-0000-0000-000000000001", |_| Ok(()))]
    #[case::short("00000000-0000-0000-0000-000000000001", |path| std::fs::write(path, b"x"))]
    #[case::directory("00000000-0000-0000-0000-000000000001", |path| std::fs::create_dir(path))]
    #[tokio::test]
    async fn baseline_spool_failures_emit_read_diagnostics(
        #[case] token: &str,
        #[case] prepare: fn(camino::Utf8PathBuf) -> std::io::Result<()>,
    ) {
        let diagnostics = baseline_spool_diagnostics(token, prepare).await;
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|event| event["code"] == "baseline.output.read")
            .collect();

        let actual = errors.len();

        assert_eq!(actual, 1, "{diagnostics:?}");
        assert_eq!(errors[0]["run_id"], "spool-read-failure");
        assert_eq!(errors[0]["kind"], "diagnostic");
    }

    #[tokio::test]
    async fn truncated_baseline_spool_reports_missing_byte_count_after_exporting_prefix() {
        let diagnostics =
            baseline_spool_diagnostics("00000000-0000-0000-0000-000000000001", |path| {
                std::fs::write(path, b"x")
            })
            .await;

        let actual = diagnostics.len();
        let actual_2 = diagnostics[1]["message"]
            .as_str()
            .unwrap()
            .contains("offset=0:\nx");

        assert_eq!(actual, 3);
        assert!(actual_2);
        assert_eq!(diagnostics[2]["code"], "baseline.output.read");
        assert_eq!(
            diagnostics[2]["message"],
            "baseline spool ended with 3 retained bytes missing"
        );
    }
}
