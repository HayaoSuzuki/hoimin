mod coverage_tests {
    use super::*;

    #[test]
    fn defaults_and_diagnostic_capture_are_observable() {
        let control = RunControl::default();
        assert!(!control.request.is_cancelled());
        let meter = TestDiskMeter(Arc::new(std::sync::Mutex::new(Box::new(FailingDiskMeter))));
        assert_eq!(format!("{meter:?}"), "TestDiskMeter { .. }");
        let mut capture = SharedDiagnosticCapture::default();
        assert_eq!(capture.write(b"diagnostic").unwrap(), 10);
        capture.flush().unwrap();
        assert_eq!(*capture.0.lock().unwrap(), b"diagnostic");
    }

    #[tokio::test]
    async fn blocking_preparation_reports_missing_targets_workspace_and_replay() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let preflight = RunEffect::Preflight(Preflight { id: EffectId(501) });
        let error = prepare_blocking_effect(&mut context, preflight.clone())
            .err()
            .unwrap();
        assert_eq!(error.failure.code(), "shell.targets.missing");
        context.resolved_targets = Some(Vec::new());
        let workspace = context.workspace.take();
        let error = prepare_blocking_effect(&mut context, preflight)
            .err()
            .unwrap();
        assert_eq!(error.failure.code(), "shell.blocking_io");
        let error = prepare_blocking_effect(
            &mut context,
            RunEffect::Cleanup(Cleanup {
                id: EffectId(502),
                reservations: Vec::new(),
            }),
        )
        .err()
        .unwrap();
        assert_eq!(error.failure.code(), "shell.blocking_io");
        context.workspace = workspace;
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
        assert!(
            matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.blocking_io")
        );
    }

    #[tokio::test]
    async fn duplicate_blocking_ownership_is_rejected_without_replacing_the_owner() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let event = accept_blocking_completion(
            &mut context,
            BlockingEffectCompletion::Candidate {
                id: EffectId(504),
                replay: CandidateReplay::new([]),
                event: Box::new(RunEvent::CancellationRequested),
            },
        );
        assert!(
            matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.blocking_io")
        );
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
        assert!(
            matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.blocking_io")
        );
        assert!(context.workspace.is_some());
    }

    #[tokio::test]
    async fn session_open_errors_preserve_missing_and_invalid_path_causes() {
        let (project, _parent, mut context, _, _) = context_with_worker().await;
        assert_eq!(
            session(&mut context, EffectId(506))
                .await
                .err()
                .unwrap()
                .failure
                .code(),
            "session.missing"
        );
        context.session_path = Some(Utf8PathBuf::from_path_buf(project.path().to_owned()).unwrap());
        assert_eq!(
            session(&mut context, EffectId(507))
                .await
                .err()
                .unwrap()
                .failure
                .code(),
            "session.open"
        );
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
        assert!(
            matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.worker.environment")
        );
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
        assert!(prepare_session_artifacts(&config).is_err());
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
        assert!(
            matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.budget.scheduler")
        );
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
        assert!(
            matches!(event, RunEvent::EffectFailed(error) if error.failure.code() == "shell.worker.environment")
        );
    }

    #[tokio::test]
    async fn resource_cleanup_reports_missing_ownership_and_panicking_start_hooks() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        let workspace = context.workspace.take();
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
        assert_eq!(
            missing.workspace.unwrap_err(),
            "workspace ownership is unavailable during final cleanup"
        );
        context.workspace = workspace;
        let failed = close_context_resources(
            &mut context,
            &budget,
            false,
            true,
            Box::new(|| panic!("cleanup hook panicked")),
        )
        .await;
        assert!(
            failed
                .workspace
                .unwrap_err()
                .contains("cleanup hook panicked")
        );
        assert!(failed.process.is_ok());
        assert!(failed.expiry.is_none());
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
        assert_eq!(finalized.warnings[0], ("previous", "preserve".into()));
        assert_eq!(finalized.warnings[1].0, "metrics.write");
        assert!(finalized.warnings[1].1.contains("metrics hook panicked"));
        assert!(finalized.expiry.is_none());
        assert!(!path.exists());
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
        assert_eq!(stop.code, "run.finalization.failed");
        assert_eq!(summary.exit_code, 2);
        apply_finalization_errors_to_summary(
            &mut summary,
            &[
                "unstructured failure".into(),
                "cleanup.failed: detail".into(),
                "cleanup.failed: detail".into(),
            ],
        );
        assert_eq!(summary.disk.stop.as_ref().unwrap().secondary.len(), 1);
        assert!(!summary.complete);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn session_artifacts_reject_unrepresentable_native_names() {
        use std::os::unix::{ffi::OsStringExt, fs::symlink};
        let project = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        for (root, expected) in [
            (project.path(), "artifact path"),
            (external.path(), "database path"),
        ] {
            let native = root.join(OsString::from_vec(b"native-\xff".to_vec()));
            std::fs::create_dir(&native).unwrap();
            let link = project.path().join("alias");
            symlink(&native, &link).unwrap();
            let mut data = config.clone().into_data();
            data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
                path: Utf8PathBuf::from_path_buf(link.join("session.sqlite3")).unwrap(),
            });
            assert!(
                prepare_session_artifacts(&RunConfig::try_from(data).unwrap())
                    .unwrap_err()
                    .contains(expected)
            );
            std::fs::remove_file(link).unwrap();
        }
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
        assert!(prepare_session_artifacts(&RunConfig::try_from(data).unwrap()).is_err());
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
        assert!(
            prepare_session_artifacts(&RunConfig::try_from(data).unwrap())
                .unwrap_err()
                .contains("literal backslash")
        );
    }
    #[tokio::test]
    async fn session_effects_without_configuration_report_the_same_missing_owner() {
        let (_project, _parent, mut context, candidate, _) = context_with_worker().await;
        let fingerprint = hoimin_core::RunFingerprint::from_bytes([1; 32]);
        let max_mutants = std::num::NonZeroUsize::new(1).unwrap();
        let effects = [
            RunEffect::LoadSession(hoimin_core::LoadSession {
                id: EffectId(511),
                fingerprint,
                max_mutants,
            }),
            RunEffect::LookupStoredResult(hoimin_core::LookupStoredResult {
                id: EffectId(512),
                worker: 0,
                run_id: "run".into(),
                mutant_id: "mutant".into(),
            }),
            RunEffect::BeginSession(hoimin_core::BeginSession {
                id: EffectId(513),
                fingerprint,
                max_mutants,
                run_id: "run".into(),
            }),
            RunEffect::PersistResult(hoimin_core::PersistResult {
                id: EffectId(514),
                worker: 0,
                result: hoimin_core::MutantResult {
                    run_id: "run".into(),
                    candidate,
                    outcome: hoimin_core::MutationOutcome::NotRun,
                    resource_mode: ResourceMode::BestEffort,
                    diagnostics: Vec::new(),
                },
            }),
            RunEffect::FinishSession(hoimin_core::FinishSession {
                id: EffectId(515),
                run_id: "run".into(),
                complete: true,
            }),
        ];
        for effect in effects {
            let expected = effect.id();
            let event = execute_effect(&mut context, effect).await;
            assert!(
                matches!(event, RunEvent::EffectFailed(error) if error.id == expected && error.failure.code() == "session.missing")
            );
        }
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
            let deadline = Instant::now() + Duration::from_secs(5);
            while observing.dispatched_effects.load(Ordering::Acquire) == 0 {
                assert!(
                    Instant::now() < deadline,
                    "run did not reach guarded dispatch"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            observing
                .cancelled_at
                .lock()
                .unwrap()
                .get_or_insert_with(tokio::time::Instant::now);
            observing.request.cancel_under_held_spawn_gate_for_tests();
            drop(gate);
        });
        assert_eq!(running.await.unwrap().unwrap(), 130);
        assert_eq!(observing.max_process_tasks(), 0);
    }
    #[tokio::test]
    async fn direct_workspace_adapter_restores_and_verifies_the_original_after_replay() {
        let (_project, _parent, mut context, candidate, retry) = context_with_worker().await;
        let id = retry.id();
        let root = context.workspace().worker(0).unwrap().root().to_owned();
        let duplicate = execute_direct_io_effect(&mut context, RunEffect::CreateWorker(retry));
        assert!(matches!(duplicate, RunEvent::EffectFailed(error) if error.id == id));
        assert_eq!(context.workspace().worker(0).unwrap().root(), &root);
        let spool_directory = tempfile::tempdir().unwrap();
        let mut store = crate::analyzer::CandidateStore::new_in(1, spool_directory.path()).unwrap();
        store.push(&candidate).unwrap();
        let spool = store.finish().unwrap();
        context.candidate_replay = Some(CandidateReplay::new([candidate.id.clone()]));
        let loaded = execute_direct_io_effect(
            &mut context,
            candidate_read_effect(520, spool, hoimin_core::CandidateReadTarget::Next),
        );
        assert!(
            matches!(loaded, RunEvent::CandidateLoaded(CandidateLoaded { candidate: Some(value), .. }) if value == candidate)
        );
        let mutated = execute_direct_io_effect(
            &mut context,
            RunEffect::ApplyMutation(ApplyMutation {
                id: EffectId(521),
                worker: 0,
                candidate,
            }),
        );
        assert!(matches!(mutated, RunEvent::MutationApplied(_)));
        let reset = execute_direct_io_effect(
            &mut context,
            RunEffect::ResetWorker(ResetWorker {
                id: EffectId(522),
                worker: 0,
            }),
        );
        assert!(matches!(reset, RunEvent::WorkerReset(_)));
        let verified = execute_direct_io_effect(
            &mut context,
            RunEffect::VerifyOriginals(VerifyOriginals {
                id: EffectId(523),
                checkpoint: IntegrityCheckpoint::PreFinalReport,
            }),
        );
        assert!(matches!(verified, RunEvent::OriginalsVerified(_)));
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"original\n"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn fixture_waiters_bound_their_failure_cleanup_and_cancel_an_unresponsive_run() {
        let mut completed = std::future::ready(17);
        assert!(
            wait_for_live_process(&mut completed, || false)
                .await
                .unwrap_err()
                .contains("finished before fixture readiness")
        );
        let mut unresponsive = std::future::pending::<()>();
        assert_eq!(
            wait_for_live_process(&mut unresponsive, || false)
                .await
                .unwrap_err(),
            "live process fixture did not become ready"
        );
        let control = RunControl::new();
        assert_eq!(
            wait_for_disk_stop(&mut unresponsive, &control)
                .await
                .unwrap_err(),
            "periodic disk stop did not interrupt the active process"
        );
        assert!(control.is_cancelled());
    }

    #[test]
    fn startup_events_do_not_establish_a_shutdown_budget() {
        let now = tokio::time::Instant::now();
        let mut budget = None;
        assert_eq!(
            establish_event_shutdown_budget(
                &mut budget,
                &RunEvent::StartRequested(StartRequested),
                now + Duration::from_secs(1),
                now,
                Duration::from_secs(1)
            ),
            None
        );
        assert_eq!(budget, None);
    }
    #[tokio::test]
    async fn shutdown_drain_keeps_the_first_outer_io_task_failure_and_clears_accounting() {
        let mut io_tasks = JoinSet::new();
        let first = io_tasks.spawn(async { panic!("first outer I/O panic") });
        while !first.is_finished() {
            tokio::task::yield_now().await;
        }
        let second = io_tasks.spawn(async { panic!("second outer I/O panic") });
        while !second.is_finished() {
            tokio::task::yield_now().await;
        }
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
        assert!(failure.contains("blocking I/O task failed while stopping"));
        assert!(failure.contains("first outer I/O panic"));
        assert!(!failure.contains("second outer I/O panic"));
        assert_eq!(in_flight, 0);
        assert!(io_tasks.is_empty());
        assert!(!expiry_reported);
        assert_eq!(warnings, Vec::<(&str, String)>::new());
    }
    #[rstest::rstest]
    #[case::first_signal_during_final_write(true, false, false)]
    #[case::first_signal_during_final_flush(true, true, false)]
    #[case::handler_failure_during_final_write(true, false, true)]
    #[case::handler_failure_during_final_flush(true, true, true)]
    #[case::first_signal_during_serial_write(false, false, false)]
    #[case::first_signal_during_serial_flush(false, true, false)]
    #[case::handler_failure_during_serial_write(false, false, true)]
    #[case::handler_failure_during_serial_flush(false, true, true)]
    #[tokio::test(flavor = "current_thread")]
    async fn interrupt_observation_preserves_owned_report_delivery(
        #[case] final_report: bool,
        #[case] pause_flush: bool,
        #[case] handler_failure: bool,
    ) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut control = RunControl::new();
        if final_report {
            control.shutdown_grace = Duration::from_millis(100);
        }
        let observed = control.clone();
        let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        let interrupts = crate::interrupt::spawn_test_monitor(signal_rx, drop);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = PausedReportWriter {
            entered: Some(entered_tx),
            release: release_rx,
            pause_flush,
            marker: if final_report {
                b"run_finished"
            } else {
                b"baseline_finished"
            },
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
            signal_tx
                .send(if handler_failure {
                    Err("install Ctrl+C handler: injected failure".to_owned())
                } else {
                    Ok(())
                })
                .unwrap();
            if !final_report {
                while !observed.finalization_events().contains(&"interrupt_observed") {
                    tokio::task::yield_now().await;
                }
                release.take().unwrap().send(()).unwrap();
            }
        };
        let completed = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(&mut run, observation)
        })
        .await;
        let roots = observed.managed_root_paths();
        let retained_delivery = roots[1].exists();
        let acknowledged = observed
            .finalization_events()
            .contains(&"output_acknowledged");
        if let Some(release) = release {
            release.send(()).unwrap();
        }
        drop(run);
        let (result, ()) = completed.unwrap();
        if handler_failure {
            assert!(
                result
                    .unwrap_err()
                    .contains("install Ctrl+C handler: injected failure")
            );
        } else if final_report {
            assert_eq!(result.unwrap_or(2), 2);
        } else {
            assert_eq!(result.unwrap(), 130);
        }
        if final_report {
            assert!(retained_delivery);
            assert!(!acknowledged);
        } else {
            assert_eq!(acknowledged, !handler_failure);
        }
    }
    #[rstest::rstest]
    #[case::delivery_completes_inside_grace(true)]
    #[case::delivery_exceeds_grace(false)]
    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_during_serial_output_preserves_the_delivery_deadline(
        #[case] release_inside_grace: bool,
    ) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut control = RunControl::new();
        control.shutdown_grace = if release_inside_grace {
            Duration::from_secs(2)
        } else {
            Duration::from_millis(100)
        };
        let cancelling = control.clone();
        let observed = control.clone();
        let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
        let (_signal_tx, signal_rx) = mpsc::unbounded_channel();
        let interrupts = crate::interrupt::spawn_test_monitor(signal_rx, drop);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = PausedReportWriter {
            entered: Some(entered_tx),
            release: release_rx,
            pause_flush: false,
            marker: b"baseline_finished",
            record: Vec::new(),
            target_seen: false,
        };
        let mut run = Box::pin(run_loop_prepared_timed(
            config, writer, Vec::new(), control, hoimin_core::RunSelection::all(),
            None, timing, interrupts,
        ));
        let mut release = Some(release_tx);
        let observation = async {
            entered_rx.await.unwrap();
            cancelling.cancel();
            if release_inside_grace {
                while !observed.finalization_events().contains(&"serial_stop_observed") {
                    tokio::task::yield_now().await;
                }
                release.take().unwrap().send(()).unwrap();
            }
        };
        let completed = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(&mut run, observation)
        }).await;
        let acknowledged = observed.finalization_events().contains(&"output_acknowledged");
        if let Some(release) = release { release.send(()).unwrap(); }
        drop(run);
        let (result, ()) = completed.unwrap();
        if release_inside_grace {
            assert_eq!(result.unwrap(), 130);
            assert!(acknowledged);
        } else {
            assert!(result.unwrap_err().contains("cancellation: shutdown grace expired"));
            assert!(!acknowledged);
        }
    }

    #[rstest::rstest]
    #[case::signal_with_cooperative_io(false, true)]
    #[case::handler_failure_with_cooperative_io(true, true)]
    #[case::signal_with_stalled_io(false, false)]
    #[case::handler_failure_with_stalled_io(true, false)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn interrupts_during_owned_materialization_preserve_primary_and_drain_evidence(
        #[case] handler_failure: bool,
        #[case] release_inside_grace: bool,
    ) {
        let project = tempfile::tempdir().unwrap();
        let metrics = tempfile::tempdir().unwrap();
        let mut data = paused_materialization_config(&project, "5s").into_data();
        data.output.metrics = Some(Utf8PathBuf::from_path_buf(metrics.path().join("metrics.json")).unwrap());
        let config = RunConfig::try_from(data).unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let grace = if release_inside_grace { Duration::from_secs(2) } else { Duration::from_millis(100) };
        let (control, mut pause) = RunControl::with_materialization_pause_and_shutdown_grace(0, grace);
        let observed = control.clone();
        let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        let interrupts = crate::interrupt::spawn_test_monitor(signal_rx, drop);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let controller = tokio::task::spawn_blocking(move || {
            pause.wait_until_entered(Duration::from_secs(2)).unwrap();
            let guard = pause.release_guard();
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            drop(guard);
        });
        let mut run = Box::pin(run_loop_prepared_timed(
            config, Vec::new(), Vec::new(), control, hoimin_core::RunSelection::all(),
            None, timing, interrupts,
        ));
        let mut release = Some(release_tx);
        let observation = async {
            entered_rx.await.unwrap();
            signal_tx.send(if handler_failure {
                Err("install Ctrl+C handler: materialization failure".to_owned())
            } else { Ok(()) }).unwrap();
            if release_inside_grace {
                while !observed.finalization_events().contains(&"interrupt_observed") {
                    tokio::task::yield_now().await;
                }
                release.take().unwrap().send(()).unwrap();
            }
        };
        let completed = tokio::time::timeout(Duration::from_secs(4), async {
            tokio::join!(&mut run, observation)
        }).await;
        if let Some(release) = release { release.send(()).unwrap(); }
        drop(run);
        controller.await.unwrap();
        let (result, ()) = completed.unwrap();
        if handler_failure {
            let error = result.unwrap_err();
            assert!(error.starts_with("install Ctrl+C handler: materialization failure"), "{error}");
            assert_eq!(error.contains("shutdown grace expired"), !release_inside_grace, "{error}");
        } else if release_inside_grace {
            assert_eq!(result.unwrap(), 130);
        } else {
            let error = result.unwrap_err();
            assert!(error.contains("cancellation: shutdown grace expired"), "{error}");
            assert!(error.contains("blocking I/O tasks: 1"), "{error}");
        }
        assert_eq!(observed.max_process_tasks(), 0);
    }

    #[rstest::rstest]
    #[case::process_owner(true)]
    #[case::blocking_io_owner(false)]
    #[tokio::test(flavor = "current_thread")]
    async fn delivered_completions_do_not_hide_failed_outer_task_joins(#[case] process: bool) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let control = RunControl::new();
        if process {
            control.process_completion_panic.store(true, Ordering::Release);
        } else {
            control.io_completion_panic.store(true, Ordering::Release);
        }
        let observed = control.clone();
        let result = run_loop_prepared(config, Vec::new(), Vec::new(), control,
            hoimin_core::RunSelection::all(), None).await;
        let error = result.unwrap_err();
        let task_kind = if process { "process task failed" } else { "blocking I/O task failed" };
        assert!(error.contains(task_kind), "{error}");
        assert!(error.contains("post-delivery panic"), "{error}");
        assert!(!error.contains("shutdown grace expired"), "{error}");
        assert!(!observed.finalization_events().contains(&"output_acknowledged"));
    }

    #[rstest::rstest]
    #[case::successful_run(false, false)]
    #[case::unavailable_destination(true, false)]
    #[case::failed_owned_task(false, true)]
    #[tokio::test(flavor = "current_thread")]
    async fn outer_metrics_finalization_preserves_run_result_and_reports_unwritten_output(
        #[case] unavailable: bool,
        #[case] process_failure: bool,
    ) {
        let project = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let path = output.path().join(if unavailable { "missing/metrics.json" } else { "metrics.json" });
        let mut data = output_failure_test_config(&project).into_data();
        data.output.metrics = Some(Utf8PathBuf::from_path_buf(path.clone()).unwrap());
        let config = RunConfig::try_from(data).unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked).unwrap();
        let control = RunControl::new();
        control.process_completion_panic.store(process_failure, Ordering::Release);
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let result = run_loop_prepared(config, stdout.clone(), stderr.clone(), control,
            hoimin_core::RunSelection::all(), None).await;
        if process_failure {
            assert!(result.unwrap_err().contains("injected process post-delivery panic"));
        } else { assert_eq!(result.unwrap(), 0); }
        let mut diagnostics = stdout.snapshot();
        diagnostics.extend(stderr.snapshot());
        let diagnostics = String::from_utf8(diagnostics).unwrap();
        if unavailable {
            assert!(!path.exists());
            assert!(diagnostics.contains("metrics.write"), "{diagnostics}");
        } else if process_failure {
            assert!(!path.exists());
            assert!(diagnostics.contains("metrics.incomplete"), "{diagnostics}");
        } else {
            let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert!(report.is_object());
            assert!(!diagnostics.contains("metrics.incomplete"));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_mutant_run_persists_its_result_and_finishes_metrics_before_releasing_roots() {
        let project = tempfile::tempdir().unwrap();
        let artifacts = tempfile::tempdir().unwrap();
        let mut data = output_failure_test_config(&project).into_data();
        std::fs::write(project.path().join("target.py"), "def compute():\n    return 1 + 2\n").unwrap();
        data.limits.max_mutants = std::num::NonZeroUsize::new(100).unwrap();
        let metrics = artifacts.path().join("metrics.json");
        let session = artifacts.path().join("session.sqlite3");
        data.output.metrics = Some(Utf8PathBuf::from_path_buf(metrics.clone()).unwrap());
        data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
            path: Utf8PathBuf::from_path_buf(session.clone()).unwrap(),
        });
        let config = RunConfig::try_from(data).unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked).unwrap();
        let control = RunControl::new();
        let observed = control.clone();
        let stdout = CollectingWriter::default();
        let result = run_loop_prepared(config, stdout.clone(), Vec::new(), control,
            hoimin_core::RunSelection::all(), None).await.unwrap();
        assert_eq!(result, 0);
        let report = String::from_utf8(stdout.snapshot()).unwrap();
        assert!(report.contains("mutant_finished"), "{report}");
        let connection = rusqlite::Connection::open(session).unwrap();
        assert!(connection.query_row("SELECT count(*) FROM results", [], |row| row.get::<_, i64>(0)).unwrap() > 0);
        assert_eq!(connection.query_row("SELECT complete FROM runs", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
        let metrics: serde_json::Value = serde_json::from_slice(&std::fs::read(metrics).unwrap()).unwrap();
        assert!(metrics.is_object());
        assert!(observed.finalization_events().contains(&"output_acknowledged"));
        assert!(observed.managed_root_paths().iter().all(|path| !path.exists()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn saved_plan_execution_preserves_selected_candidate_membership() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        std::fs::write(project.path().join("target.py"), "def compute():\n    return 1 + 2\n").unwrap();
        let output = crate::plan::create(config,
            hoimin_core::DiscoveryConfig::new(Duration::from_secs(10)).unwrap()).await.unwrap();
        let selected = output.manifest.candidates[0].id.clone();
        let artifact = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(artifact.path(), serde_json::to_vec(&output.manifest).unwrap()).unwrap();
        let verified = crate::plan::prepare_verify(artifact.path(), std::slice::from_ref(&selected)).await.unwrap();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        assert_eq!(verified.execute(stdout.clone(), stderr.clone()).await.unwrap(), 0);
        let events: Vec<OutputEvent> = String::from_utf8(stdout.snapshot()).unwrap().lines()
            .map(|line| serde_json::from_str(line).unwrap()).collect();
        let finished: Vec<_> = events.iter().filter_map(|event| match event {
            OutputEvent::MutantFinished(value) => Some(value),
            _ => None,
        }).collect();
        assert_eq!(finished.len(), 1);
        let event = serde_json::to_string(finished[0]).unwrap();
        assert!(event.contains(&selected), "{event}");
        assert_eq!(stderr.snapshot(), Vec::<u8>::new());
    }

}
