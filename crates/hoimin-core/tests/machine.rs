use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use hoimin_core::{
    AnalysisFinished, ByteSpan, CandidateLoaded, CandidateSpoolRef, CleanupFinished, CommandArg,
    EffectFailed, EffectId, MutationApplied, MutationCandidate, MutationProfile, MutationStatus,
    MutationSummary, OriginalsVerified, OutputConfig, OutputEmitted, OutputEvent,
    PreflightCompleted, ProcessFinished, ProcessTermination, RawRunConfig, RawRunLimits,
    ResourceMode, ResultPersisted, RunConfig, RunEffect, RunEvent, RunFingerprint, RunPhase,
    RunState, SessionFinished, SessionLoaded, SessionResumeRef, SessionStarted, StartRequested,
    StoredResult, StoredResultLoaded, TargetSlice, TargetsResolved, WorkerCreated, WorkerReset,
    transition,
};

#[test]
fn baseline_success_requests_analysis_without_performing_io() {
    let (state, effects) = start_state();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: resolve_id,
            targets: vec![TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            }],
        }),
    )
    .unwrap();
    let preflight_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Preflight(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::PreflightCompleted(PreflightCompleted {
            id: preflight_id,
            per_worker_logical_bytes: 10,
            requested_workers: 1,
            aggregate_logical_bytes: 10,
            fingerprint: None,
        }),
    )
    .unwrap();
    let (state, effects) = complete_run_started(state, &effects);
    let worker_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::CreateWorker(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::WorkerCreated(WorkerCreated {
            id: worker_id,
            worker: 0,
            reservation_id: reservation_id(find_effect(&effects, |effect| {
                matches!(effect, RunEffect::CreateWorker(_))
            })),
        }),
    )
    .unwrap();
    let baseline_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunBaseline(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(baseline_id, ProcessTermination::Exit(0))),
    )
    .unwrap();
    assert_eq!(next.phase(), RunPhase::Analyze);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::AnalyzeFile(_)))
    );
}

#[test]
fn run_started_emits_normalized_focused_profile() {
    let state = RunState::new("run-1", focused_config());
    let (state, effects) = transition(state, RunEvent::StartRequested(StartRequested)).unwrap();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: resolve_id,
            targets: vec![TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            }],
        }),
    )
    .unwrap();
    let preflight_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Preflight(_))
    }));
    let (_, effects) = transition(
        state,
        RunEvent::PreflightCompleted(PreflightCompleted {
            id: preflight_id,
            per_worker_logical_bytes: 10,
            requested_workers: 1,
            aggregate_logical_bytes: 10,
            fingerprint: None,
        }),
    )
    .unwrap();

    let RunEffect::EmitOutput(output) = effects.first().unwrap() else {
        unreachable!()
    };
    let OutputEvent::RunStarted(run_started) = &output.event else {
        unreachable!()
    };
    assert_eq!(
        run_started.normalized_config.as_ref().unwrap().profile,
        MutationProfile::Focused
    );
}

#[test]
fn baseline_failure_runs_no_mutants_and_selects_exit_three() {
    let (state, effects) = waiting_for_baseline();
    let baseline_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunBaseline(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(baseline_id, ProcessTermination::Exit(1))),
    )
    .unwrap();

    assert_eq!(next.phase(), RunPhase::Finalize);
    assert_eq!(next.exit_code(), 3);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
    );
}

#[test]
fn empty_candidate_spool_finishes_with_null_score() {
    let (state, effects) = waiting_for_analysis();
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "empty".to_owned(),
                records: 0,
            }),
            truncated: false,
        }),
    )
    .unwrap();

    assert_eq!(next.phase(), RunPhase::Finalize);
    assert_eq!(next.summary(), &MutationSummary::default());
    assert_eq!(next.exit_code(), 0);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
    );
}

#[test]
fn pending_effects_reject_unknown_duplicate_and_wrong_completion_kind() {
    let (state, effects) = start_state();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let unknown = transition(
        state.clone(),
        RunEvent::OutputEmitted(OutputEmitted { id: EffectId(999) }),
    )
    .unwrap_err();
    assert_eq!(unknown.code(), "machine.effect.unknown");

    let wrong = transition(
        state.clone(),
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: resolve_id,
            spool: None,
            truncated: false,
        }),
    )
    .unwrap_err();
    assert_eq!(wrong.code(), "machine.effect.wrong_completion");

    let completed = TargetsResolved {
        id: resolve_id,
        targets: Vec::new(),
    };
    let (state, _) = transition(state, RunEvent::TargetsResolved(completed.clone())).unwrap();
    let duplicate = transition(state, RunEvent::TargetsResolved(completed)).unwrap_err();
    assert_eq!(duplicate.code(), "machine.effect.duplicate");
}

#[test]
fn retired_late_completion_is_distinct_from_a_true_duplicate() {
    let (state, effects) = waiting_for_candidate();
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let completed = CandidateLoaded {
        id: read_id,
        worker: 0,
        candidate: Some(fixture_candidate(1)),
        next_offset: 1,
    };
    let (state, effects) = transition(state, RunEvent::CandidateLoaded(completed.clone())).unwrap();
    let duplicate = transition(state.clone(), RunEvent::CandidateLoaded(completed)).unwrap_err();
    assert_eq!(duplicate.code(), "machine.effect.duplicate");

    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (stopped, _) = transition(state, RunEvent::CancellationRequested).unwrap();
    assert!(stopped.is_effect_retired(apply_id));
    let retired = transition(
        stopped,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap_err();
    assert_eq!(retired.code(), "machine.effect.retired");
}

#[test]
fn deadline_and_cancellation_stop_scheduling_new_mutants() {
    for (event, exit_code) in [
        (RunEvent::DeadlineReached, 4),
        (RunEvent::CancellationRequested, 130),
    ] {
        let (state, _) = waiting_for_baseline();
        let (next, effects) = transition(state, event).unwrap();
        assert_eq!(next.phase(), RunPhase::Finalize);
        assert_eq!(next.exit_code(), exit_code);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
        );
    }
}

#[test]
fn deadline_before_preflight_still_emits_a_complete_report_before_cleanup() {
    let (state, _) = start_state();
    let (state, effects) = transition(state, RunEvent::DeadlineReached).unwrap();
    let started_id = effect_id(find_effect(
        &effects,
        |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::RunStarted(_))),
    ));
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: started_id }),
    )
    .unwrap();
    let cleanup = find_effect(&effects, |effect| matches!(effect, RunEffect::Cleanup(_)));
    let RunEffect::Cleanup(cleanup) = cleanup else {
        unreachable!()
    };
    let (state, effects) = transition(
        state,
        RunEvent::CleanupFinished(hoimin_core::CleanupFinished {
            id: cleanup.id,
            released_reservations: cleanup.reservations.clone(),
        }),
    )
    .unwrap();

    assert_eq!(state.exit_code(), 4);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RunEffect::EmitOutput(value)
            if matches!(&value.event, hoimin_core::OutputEvent::RunFinished(_))
    )));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::CreateWorker(_)))
    );
}

#[test]
fn cancellation_flushes_active_and_remaining_candidates_as_not_run() {
    let (state, effects) = waiting_for_analysis();
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "cancel-drain".to_owned(),
                records: 3,
            }),
            truncated: false,
        }),
    )
    .unwrap();
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, _) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(fixture_candidate(1)),
            next_offset: 10,
        }),
    )
    .unwrap();
    let (mut state, mut effects) = transition(state, RunEvent::CancellationRequested).unwrap();

    for sequence in 1..=3 {
        if sequence != 1 {
            let read_id = effect_id(find_effect(&effects, |effect| {
                matches!(effect, RunEffect::ReadCandidate(_))
            }));
            (state, effects) = transition(
                state,
                RunEvent::CandidateLoaded(CandidateLoaded {
                    id: read_id,
                    worker: 0,
                    candidate: Some(fixture_candidate(sequence)),
                    next_offset: sequence * 10,
                }),
            )
            .unwrap();
        }
        (state, effects) = complete_mutant_started(state, &effects);
        let finished = find_effect(&effects, |effect| {
            matches!(effect, RunEffect::EmitOutput(value)
                if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(value)
                    if value.status == MutationStatus::NotRun
                        && value.candidate.sequence == sequence))
        });
        let finished_id = effect_id(finished);
        (state, effects) = transition(
            state,
            RunEvent::OutputEmitted(OutputEmitted { id: finished_id }),
        )
        .unwrap();
        assert_eq!(state.summary().not_run, sequence);
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            RunEffect::ApplyMutation(_) | RunEffect::RunMutant(_)
        )));
    }

    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: None,
            next_offset: 30,
        }),
    )
    .unwrap();
    assert_eq!(state.phase(), RunPhase::Finalize);
    assert_eq!(state.exit_code(), 130);
    assert!(
        effects
            .iter()
            .any(|effect| { matches!(effect, RunEffect::VerifyOriginals(_)) })
    );
}

#[test]
fn one_active_candidate_is_applied_classified_and_reported() {
    let (state, effects) = waiting_for_candidate();
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let candidate = fixture_candidate(1);
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(candidate.clone()),
            next_offset: 17,
        }),
    )
    .unwrap();
    assert_eq!(state.candidate_offset(), 17);
    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap();
    let (state, effects) = complete_mutant_started(state, &effects);
    let mutant_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunMutant(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::MutantFinished(process_finished(mutant_id, ProcessTermination::Exit(1))),
    )
    .unwrap();

    assert_eq!(next.summary().killed, 1);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RunEffect::EmitOutput(value)
            if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(value)
                if value.status == MutationStatus::Killed && value.candidate == candidate)
    )));
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the filtered candidate lifecycle verifies each ordered state-machine transition"
)]
fn candidate_filter_skips_unrequested_candidates() {
    let first = fixture_candidate(1);
    let second = fixture_candidate(2);
    let (state, effects) = waiting_for_filtered_analysis(BTreeSet::from([second.id.clone()]));
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "filtered".to_owned(),
                records: 2,
            }),
            truncated: false,
        }),
    )
    .unwrap();
    let first_read = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));

    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: first_read,
            worker: 0,
            candidate: Some(first),
            next_offset: 10,
        }),
    )
    .unwrap();

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::ReadCandidate(_)))
    );
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        RunEffect::ApplyMutation(_) | RunEffect::RunMutant(_) | RunEffect::EmitOutput(_)
    )));

    let second_read = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: second_read,
            worker: 0,
            candidate: Some(second.clone()),
            next_offset: 20,
        }),
    )
    .unwrap();

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::ApplyMutation(_)))
    );

    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap();
    let started = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(value)
            if matches!(&value.event, OutputEvent::MutantStarted(event) if event.mutant_id == second.id))
    });
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted {
            id: effect_id(started),
        }),
    )
    .unwrap();
    let mutant_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunMutant(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutantFinished(process_finished(mutant_id, ProcessTermination::Exit(1))),
    )
    .unwrap();
    let finished = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(value)
            if matches!(&value.event, OutputEvent::MutantFinished(event) if event.candidate.id == second.id))
    });
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted {
            id: effect_id(finished),
        }),
    )
    .unwrap();
    let reset_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResetWorker(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::WorkerReset(hoimin_core::WorkerReset {
            id: reset_id,
            worker: 0,
        }),
    )
    .unwrap();
    let final_read = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, _) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: final_read,
            worker: 0,
            candidate: None,
            next_offset: 20,
        }),
    )
    .unwrap();
    assert_eq!(state.summary().killed, 1);
    assert_eq!(state.summary().not_run, 0);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the completion-order matrix is intentionally kept in one test"
)]
fn four_jobs_fill_four_independent_worker_chains_in_every_completion_order() {
    let mut raw = fixture_raw_config();
    raw.limits.jobs = 4;
    let (state, effects) = waiting_for_analysis_with(RunConfig::try_from(raw).unwrap());
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (mut state, mut effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "parallel".to_owned(),
                records: 4,
            }),
            truncated: false,
        }),
    )
    .unwrap();
    let mut running_workers = BTreeSet::new();
    let mut running_ids = BTreeMap::new();
    for sequence in 1..=4 {
        let read = find_effect(&effects, |effect| {
            matches!(effect, RunEffect::ReadCandidate(_))
        });
        let (read_id, worker) = match read {
            RunEffect::ReadCandidate(value) => (value.id, value.worker),
            _ => unreachable!(),
        };
        let (next, produced) = transition(
            state,
            RunEvent::CandidateLoaded(CandidateLoaded {
                id: read_id,
                worker,
                candidate: Some(fixture_candidate(sequence)),
                next_offset: sequence,
            }),
        )
        .unwrap();
        state = next;
        let apply = find_effect(&produced, |effect| {
            matches!(effect, RunEffect::ApplyMutation(_))
        });
        let apply_id = apply.id();
        let (next, started) = transition(
            state,
            RunEvent::MutationApplied(MutationApplied {
                id: apply_id,
                worker,
            }),
        )
        .unwrap();
        state = next;
        let (next, running) = complete_mutant_started(state, &started);
        state = next;
        let process = find_effect(&running, |effect| matches!(effect, RunEffect::RunMutant(_)));
        let RunEffect::RunMutant(process) = process else {
            unreachable!()
        };
        assert_eq!(process.worker, Some(worker));
        running_workers.insert(worker);
        running_ids.insert(worker, process.id);
        effects = produced;
    }
    assert_eq!(running_workers, BTreeSet::from([0, 1, 2, 3]));
    assert_eq!(state.pending_count(), 4);

    let base_state = state;
    let expected_candidates = vec![
        ("m1".to_owned(), MutationStatus::Killed),
        ("m2".to_owned(), MutationStatus::Killed),
        ("m3".to_owned(), MutationStatus::Killed),
        ("m4".to_owned(), MutationStatus::Killed),
    ];
    let mut tested_orders = 0;
    for first in 0..4 {
        for second in 0..4 {
            for third in 0..4 {
                for fourth in 0..4 {
                    let order = [first, second, third, fourth];
                    if order.into_iter().collect::<BTreeSet<_>>().len() != 4 {
                        continue;
                    }
                    tested_orders += 1;
                    let mut state = base_state.clone();
                    let mut completion_sequences = Vec::new();
                    let mut finished_candidates = Vec::new();
                    for worker in order {
                        let mut finished =
                            process_finished(running_ids[&worker], ProcessTermination::Exit(1));
                        finished.worker = Some(worker);
                        let (next, produced) =
                            transition(state, RunEvent::MutantFinished(finished)).unwrap();
                        state = next;
                        let output = find_effect(
                            &produced,
                            |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(_))),
                        );
                        let RunEffect::EmitOutput(output) = output else {
                            unreachable!()
                        };
                        let hoimin_core::OutputEvent::MutantFinished(finished) = &output.event
                        else {
                            unreachable!()
                        };
                        assert_eq!(finished.candidate.sequence, u64::from(worker) + 1);
                        assert_eq!(finished.candidate.id, format!("m{}", worker + 1));
                        completion_sequences.push(finished.sequence);
                        finished_candidates.push((finished.candidate.id.clone(), finished.status));
                    }
                    assert!(
                        completion_sequences
                            .windows(2)
                            .all(|pair| pair[0] < pair[1]),
                        "non-monotonic output sequence for completion order {order:?}"
                    );
                    finished_candidates.sort_by(|left, right| left.0.cmp(&right.0));
                    assert_eq!(finished_candidates, expected_candidates);
                    assert_eq!(state.summary().killed, 4);
                }
            }
        }
    }
    assert_eq!(tested_orders, 24);
}

#[test]
fn analyzer_failure_is_fatal_and_stops_mutant_scheduling() {
    let (state, effects) = waiting_for_analysis();
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            analysis_id,
            "analyzer.failed",
            "fixture",
        )),
    )
    .unwrap();

    assert_eq!(next.phase(), RunPhase::Finalize);
    assert_eq!(next.exit_code(), 2);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
    );
}

#[test]
fn cleanup_failure_terminates_without_reemitting_cleanup() {
    let (state, effects) = waiting_for_analysis();
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            analysis_id,
            "analyzer.failed",
            "fixture",
        )),
    )
    .unwrap();
    let diagnostic_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(value)
            if matches!(&value.event, hoimin_core::OutputEvent::Diagnostic(_)))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: diagnostic_id }),
    )
    .unwrap();
    let cleanup_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Cleanup(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            cleanup_id,
            "workspace.cleanup",
            "fixture",
        )),
    )
    .unwrap();

    let cleanup_diagnostic_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(value)
            if matches!(&value.event, hoimin_core::OutputEvent::Diagnostic(value)
                if value.code == "workspace.cleanup"))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted {
            id: cleanup_diagnostic_id,
        }),
    )
    .unwrap();
    let report = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(value)
            if matches!(&value.event, hoimin_core::OutputEvent::RunFinished(_)))
    });
    let RunEffect::EmitOutput(report) = report else {
        unreachable!()
    };
    let hoimin_core::OutputEvent::RunFinished(summary) = &report.event else {
        unreachable!()
    };
    assert_eq!(summary.exit_code, 2);
    assert!(!summary.complete);
    assert_eq!(state.phase(), RunPhase::Finalize);
    assert_eq!(state.exit_code(), 2);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::Cleanup(_)))
    );
}

#[test]
fn cleanup_failure_keeps_the_session_incomplete_before_final_reporting() {
    let mut raw = fixture_raw_config();
    raw.session = Some(hoimin_core::SessionConfig {
        path: "session.sqlite3".into(),
    });
    let (state, effects) = waiting_for_analysis_with(RunConfig::try_from(raw).unwrap());
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            analysis_id,
            "analyzer.failed",
            "fixture",
        )),
    )
    .unwrap();
    let diagnostic_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: diagnostic_id }),
    )
    .unwrap();
    let cleanup_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Cleanup(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            cleanup_id,
            "workspace.cleanup",
            "fixture",
        )),
    )
    .unwrap();
    let diagnostic_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(_))
    }));
    let (_, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: diagnostic_id }),
    )
    .unwrap();

    assert!(effects.iter().any(|effect| matches!(
        effect,
        RunEffect::FinishSession(value) if !value.complete && value.run_id == "session-run"
    )));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        RunEffect::EmitOutput(value)
            if matches!(&value.event, hoimin_core::OutputEvent::RunFinished(_))
    )));
}

#[test]
fn copy_failure_is_fatal_and_never_schedules_a_process() {
    let (state, effects) = start_state();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: resolve_id,
            targets: vec![TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            }],
        }),
    )
    .unwrap();
    let preflight_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Preflight(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            preflight_id,
            "workspace.copy.limit",
            "fixture",
        )),
    )
    .unwrap();

    assert_eq!(next.exit_code(), 2);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunBaseline(_) | RunEffect::RunMutant(_)))
    );
}

#[test]
fn pre_start_failure_emits_run_started_then_one_machine_readable_diagnostic() {
    let (state, effects) = start_state();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(resolve_id, "target.resolve", "fixture")),
    )
    .unwrap();
    let started_id = effect_id(find_effect(
        &effects,
        |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::RunStarted(_))),
    ));
    let (_, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: started_id }),
    )
    .unwrap();

    assert!(effects.iter().any(|effect| matches!(
        effect,
        RunEffect::EmitOutput(value)
            if matches!(&value.event, hoimin_core::OutputEvent::Diagnostic(value)
                if value.code == "target.resolve")
    )));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        RunEffect::CreateWorker(_) | RunEffect::RunBaseline(_)
    )));
}

#[test]
fn resumed_session_id_becomes_the_report_run_id_before_worker_creation() {
    let mut raw = fixture_raw_config();
    raw.session = Some(hoimin_core::SessionConfig {
        path: "session.sqlite3".into(),
    });
    raw.resume = true;
    let (state, _) = waiting_for_baseline_with(RunConfig::try_from(raw).unwrap());

    assert_eq!(state.run_id(), "session-run");
}

#[test]
fn reset_failure_is_fatal_and_never_schedules_the_next_mutant() {
    let (state, effects) = waiting_for_reset(ProcessTermination::Exit(1));
    let reset_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResetWorker(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            reset_id,
            "workspace.restore",
            "fixture",
        )),
    )
    .unwrap();

    assert_eq!(next.exit_code(), 2);
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        RunEffect::ReadCandidate(_) | RunEffect::RunMutant(_)
    )));
}

#[test]
fn original_modification_is_fatal_before_the_final_report() {
    let (state, effects) = waiting_for_analysis();
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "empty".to_owned(),
                records: 0,
            }),
            truncated: false,
        }),
    )
    .unwrap();
    let verify_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::VerifyOriginals(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            verify_id,
            "workspace.original.changed",
            "fixture",
        )),
    )
    .unwrap();

    assert_eq!(next.exit_code(), 2);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::EmitOutput(_)))
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
    );
}

#[test]
fn session_result_is_persisted_before_finished_output_and_reset() {
    let (state, effects) = waiting_for_session_candidate(false);
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(fixture_candidate(1)),
            next_offset: 1,
        }),
    )
    .unwrap();
    let lookup_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::LookupStoredResult(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::StoredResultLoaded(StoredResultLoaded {
            id: lookup_id,
            worker: 0,
            result: None,
        }),
    )
    .unwrap();
    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap();
    let (state, effects) = complete_mutant_started(state, &effects);
    let mutant_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunMutant(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutantFinished(process_finished(mutant_id, ProcessTermination::Exit(1))),
    )
    .unwrap();

    assert_eq!(state.summary().killed, 0);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::PersistResult(_)))
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::EmitOutput(_) | RunEffect::ResetWorker(_)))
    );
    let persist_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::PersistResult(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::ResultPersisted(ResultPersisted {
            id: persist_id,
            worker: 0,
            run_id: "session-run".to_owned(),
            mutant_id: fixture_candidate(1).id,
        }),
    )
    .unwrap();
    assert_eq!(state.summary().killed, 1);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::EmitOutput(_)))
    );
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the complete session lifecycle verifies one timeout regression"
)]
fn timeout_marks_the_session_and_final_report_incomplete() {
    let (state, effects) = waiting_for_session_candidate(false);
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let candidate = fixture_candidate(1);
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(candidate.clone()),
            next_offset: 1,
        }),
    )
    .unwrap();
    let lookup_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::LookupStoredResult(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::StoredResultLoaded(StoredResultLoaded {
            id: lookup_id,
            worker: 0,
            result: None,
        }),
    )
    .unwrap();
    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap();
    let (state, effects) = complete_mutant_started(state, &effects);
    let mutant_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunMutant(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutantFinished(process_finished(mutant_id, ProcessTermination::Timeout)),
    )
    .unwrap();
    let persist_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::PersistResult(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::ResultPersisted(ResultPersisted {
            id: persist_id,
            worker: 0,
            run_id: "session-run".to_owned(),
            mutant_id: candidate.id,
        }),
    )
    .unwrap();
    let finished_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(value)
            if matches!(&value.event, OutputEvent::MutantFinished(_)))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: finished_id }),
    )
    .unwrap();
    let reset_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResetWorker(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::WorkerReset(WorkerReset {
            id: reset_id,
            worker: 0,
        }),
    )
    .unwrap();
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: None,
            next_offset: 1,
        }),
    )
    .unwrap();
    let RunEffect::VerifyOriginals(verify) = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::VerifyOriginals(_))
    }) else {
        unreachable!()
    };
    let (state, effects) = transition(
        state,
        RunEvent::OriginalsVerified(OriginalsVerified {
            id: verify.id,
            checkpoint: verify.checkpoint.clone(),
        }),
    )
    .unwrap();
    let RunEffect::Cleanup(cleanup) =
        find_effect(&effects, |effect| matches!(effect, RunEffect::Cleanup(_)))
    else {
        unreachable!()
    };
    let (state, effects) = transition(
        state,
        RunEvent::CleanupFinished(CleanupFinished {
            id: cleanup.id,
            released_reservations: cleanup.reservations.clone(),
        }),
    )
    .unwrap();
    let RunEffect::FinishSession(finish) = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::FinishSession(_))
    }) else {
        unreachable!()
    };
    assert!(!finish.complete);

    let (_state, effects) = transition(
        state,
        RunEvent::SessionFinished(SessionFinished {
            id: finish.id,
            run_id: finish.run_id.clone(),
            complete: finish.complete,
        }),
    )
    .unwrap();
    let RunEffect::EmitOutput(output) = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(value)
            if matches!(&value.event, OutputEvent::RunFinished(_)))
    }) else {
        unreachable!()
    };
    let OutputEvent::RunFinished(summary) = &output.event else {
        unreachable!()
    };
    assert!(!summary.complete);
    assert_eq!(summary.exit_code, 4);
    assert_eq!(summary.counts.timeout, 1);
}

#[test]
fn resumed_determinate_result_is_reused_without_mutant_execution() {
    let (state, effects) = waiting_for_session_candidate(true);
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let candidate = fixture_candidate(1);
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(candidate.clone()),
            next_offset: 1,
        }),
    )
    .unwrap();
    let lookup_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::LookupStoredResult(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::StoredResultLoaded(StoredResultLoaded {
            id: lookup_id,
            worker: 0,
            result: Some(StoredResult {
                mutant_id: candidate.id,
                status: MutationStatus::Killed,
            }),
        }),
    )
    .unwrap();
    let (next, effects) = complete_mutant_started(next, &effects);
    let finished_id = effect_id(find_effect(
        &effects,
        |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(_))),
    ));
    let (next, effects) = transition(
        next,
        RunEvent::OutputEmitted(OutputEmitted { id: finished_id }),
    )
    .unwrap();

    assert_eq!(next.summary().killed, 1);
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        RunEffect::ApplyMutation(_) | RunEffect::RunMutant(_)
    )));
}

#[test]
fn reused_result_is_counted_only_after_finished_output_succeeds() {
    let (state, effects) = waiting_for_session_candidate(true);
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let candidate = fixture_candidate(1);
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(candidate.clone()),
            next_offset: 1,
        }),
    )
    .unwrap();
    let lookup_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::LookupStoredResult(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::StoredResultLoaded(StoredResultLoaded {
            id: lookup_id,
            worker: 0,
            result: Some(StoredResult {
                mutant_id: candidate.id,
                status: MutationStatus::Killed,
            }),
        }),
    )
    .unwrap();
    let (state, effects) = complete_mutant_started(state, &effects);
    let finished_id = effect_id(find_effect(
        &effects,
        |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(_))),
    ));
    let (failed, _) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(finished_id, "report.write", "fixture")),
    )
    .unwrap();

    assert_eq!(failed.summary().killed, 0);
    assert_eq!(failed.exit_code(), 2);
}

#[test]
fn session_completion_for_another_mutant_is_rejected() {
    let (state, effects) = waiting_for_session_candidate(true);
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(fixture_candidate(1)),
            next_offset: 1,
        }),
    )
    .unwrap();
    let lookup_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::LookupStoredResult(_))
    }));
    let error = transition(
        state,
        RunEvent::StoredResultLoaded(StoredResultLoaded {
            id: lookup_id,
            worker: 0,
            result: Some(StoredResult {
                mutant_id: "another-mutant".to_owned(),
                status: MutationStatus::Killed,
            }),
        }),
    )
    .unwrap_err();

    assert_eq!(error.code(), "machine.session.identity_mismatch");
}

#[test]
fn session_save_failure_is_fatal_and_does_not_schedule_reset_or_next_mutant() {
    let (state, effects) = waiting_for_session_candidate(false);
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(fixture_candidate(1)),
            next_offset: 1,
        }),
    )
    .unwrap();
    let lookup_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::LookupStoredResult(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::StoredResultLoaded(StoredResultLoaded {
            id: lookup_id,
            worker: 0,
            result: None,
        }),
    )
    .unwrap();
    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap();
    let (state, effects) = complete_mutant_started(state, &effects);
    let mutant_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunMutant(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutantFinished(process_finished(mutant_id, ProcessTermination::Exit(1))),
    )
    .unwrap();
    let persist_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::PersistResult(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            persist_id,
            "session.persist",
            "fixture",
        )),
    )
    .unwrap();

    assert_eq!(next.exit_code(), 2);
    assert_eq!(next.summary().killed, 0);
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        RunEffect::ResetWorker(_) | RunEffect::ReadCandidate(_) | RunEffect::RunMutant(_)
    )));
}

#[test]
fn mutant_process_is_only_scheduled_after_started_output_succeeds() {
    let (state, effects) = waiting_for_candidate();
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(fixture_candidate(1)),
            next_offset: 1,
        }),
    )
    .unwrap();
    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap();
    let output_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(_))
    }));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
    );

    let (_, produced) = transition(
        state.clone(),
        RunEvent::OutputEmitted(OutputEmitted { id: output_id }),
    )
    .unwrap();
    assert!(
        produced
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
    );

    let (_next, produced) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(output_id, "report.write", "fixture")),
    )
    .unwrap();

    assert!(
        !produced
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
    );
}

#[test]
fn candidate_overflow_stops_before_any_mutant_execution() {
    let (state, effects) = waiting_for_analysis();
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "overflow".to_owned(),
                records: 100,
            }),
            truncated: true,
        }),
    )
    .unwrap();

    assert_eq!(next.phase(), RunPhase::Finalize);
    assert_eq!(next.exit_code(), 4);
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        RunEffect::ReadCandidate(_) | RunEffect::RunMutant(_)
    )));
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the ordered target replay is clearer as one scenario"
)]
fn multiple_target_files_are_analyzed_in_order_before_candidate_replay() {
    let (state, effects) = start_state();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let targets = vec![
        TargetSlice {
            path: "src/a.py".into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
        TargetSlice {
            path: "src/b.py".into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
    ];
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: resolve_id,
            targets,
        }),
    )
    .unwrap();
    let preflight_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Preflight(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::PreflightCompleted(PreflightCompleted {
            id: preflight_id,
            per_worker_logical_bytes: 10,
            requested_workers: 1,
            aggregate_logical_bytes: 10,
            fingerprint: None,
        }),
    )
    .unwrap();
    let (state, effects) = complete_run_started(state, &effects);
    let create = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::CreateWorker(_))
    });
    let (state, effects) = transition(
        state,
        RunEvent::WorkerCreated(WorkerCreated {
            id: effect_id(create),
            worker: 0,
            reservation_id: reservation_id(create),
        }),
    )
    .unwrap();
    let baseline_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunBaseline(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(baseline_id, ProcessTermination::Exit(0))),
    )
    .unwrap();
    let RunEffect::AnalyzeFile(first) = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }) else {
        unreachable!()
    };
    assert_eq!(first.target.path, camino::Utf8PathBuf::from("src/a.py"));
    assert!(!first.final_target);
    let (state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: first.id,
            spool: None,
            truncated: false,
        }),
    )
    .unwrap();
    let RunEffect::AnalyzeFile(second) = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }) else {
        unreachable!()
    };
    assert_eq!(second.target.path, camino::Utf8PathBuf::from("src/b.py"));
    assert!(second.final_target);
    let (next, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: second.id,
            spool: Some(CandidateSpoolRef {
                token: "all".to_owned(),
                records: 2,
            }),
            truncated: true,
        }),
    )
    .unwrap();
    assert_eq!(next.phase(), RunPhase::Finalize);
    assert_eq!(next.exit_code(), 4);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::ReadCandidate(_)))
    );
}

#[test]
fn filtered_analysis_advances_across_nonfinal_targets_before_receiving_a_spool() {
    let (state, effects) = transition(
        RunState::with_candidate_filter(
            "run-1",
            fixture_config(),
            BTreeSet::from(["m1_selected".to_owned()]),
        ),
        RunEvent::StartRequested(StartRequested),
    )
    .unwrap();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: resolve_id,
            targets: vec![
                TargetSlice {
                    path: "src/a.py".into(),
                    lines: Vec::new(),
                    symbols: Vec::new(),
                },
                TargetSlice {
                    path: "src/b.py".into(),
                    lines: Vec::new(),
                    symbols: Vec::new(),
                },
            ],
        }),
    )
    .unwrap();
    let preflight_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Preflight(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::PreflightCompleted(PreflightCompleted {
            id: preflight_id,
            per_worker_logical_bytes: 10,
            requested_workers: 1,
            aggregate_logical_bytes: 10,
            fingerprint: None,
        }),
    )
    .unwrap();
    let (state, effects) = complete_run_started(state, &effects);
    let create = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::CreateWorker(_))
    });
    let (state, effects) = transition(
        state,
        RunEvent::WorkerCreated(WorkerCreated {
            id: effect_id(create),
            worker: 0,
            reservation_id: reservation_id(create),
        }),
    )
    .unwrap();
    let baseline_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunBaseline(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(baseline_id, ProcessTermination::Exit(0))),
    )
    .unwrap();
    let RunEffect::AnalyzeFile(first) = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }) else {
        unreachable!()
    };
    assert!(!first.final_target);

    let (next, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: first.id,
            spool: None,
            truncated: false,
        }),
    )
    .unwrap();

    assert_eq!(next.phase(), RunPhase::Analyze);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RunEffect::AnalyzeFile(value) if value.target.path == "src/b.py" && value.final_target
    )));
}

#[test]
fn every_process_termination_is_classified_by_the_machine() {
    for (termination, expected) in [
        (ProcessTermination::Exit(0), MutationStatus::Survived),
        (ProcessTermination::Exit(1), MutationStatus::Killed),
        (ProcessTermination::Timeout, MutationStatus::Timeout),
        (ProcessTermination::OutOfMemory, MutationStatus::OutOfMemory),
        (
            ProcessTermination::ProcessLimit,
            MutationStatus::ProcessLimit,
        ),
        (ProcessTermination::Cancelled, MutationStatus::NotRun),
    ] {
        let (state, effects) = waiting_for_candidate();
        let read_id = effect_id(find_effect(&effects, |effect| {
            matches!(effect, RunEffect::ReadCandidate(_))
        }));
        let (state, effects) = transition(
            state,
            RunEvent::CandidateLoaded(CandidateLoaded {
                id: read_id,
                worker: 0,
                candidate: Some(fixture_candidate(1)),
                next_offset: 1,
            }),
        )
        .unwrap();
        let apply_id = effect_id(find_effect(&effects, |effect| {
            matches!(effect, RunEffect::ApplyMutation(_))
        }));
        let (state, effects) = transition(
            state,
            RunEvent::MutationApplied(MutationApplied {
                id: apply_id,
                worker: 0,
            }),
        )
        .unwrap();
        let (state, effects) = complete_mutant_started(state, &effects);
        let mutant_id = effect_id(find_effect(&effects, |effect| {
            matches!(effect, RunEffect::RunMutant(_))
        }));
        let (_, effects) = transition(
            state,
            RunEvent::MutantFinished(process_finished(mutant_id, termination)),
        )
        .unwrap();
        assert!(effects.iter().any(|effect| matches!(
            effect,
            RunEffect::EmitOutput(value)
                if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(value)
                    if value.status == expected)
        )));
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the bounded scheduling scenario is kept as one end-to-end test"
)]
fn max_mutants_reports_remaining_candidates_as_not_run_in_stable_order() {
    let mut raw = fixture_raw_config();
    raw.limits.max_mutants = 1;
    let (state, effects) = waiting_for_analysis_with(RunConfig::try_from(raw).unwrap());
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "two".to_owned(),
                records: 2,
            }),
            truncated: false,
        }),
    )
    .unwrap();
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(fixture_candidate(1)),
            next_offset: 10,
        }),
    )
    .unwrap();
    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap();
    let (state, effects) = complete_mutant_started(state, &effects);
    let mutant_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunMutant(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutantFinished(process_finished(mutant_id, ProcessTermination::Exit(1))),
    )
    .unwrap();
    let finished_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: finished_id }),
    )
    .unwrap();
    let reset_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResetWorker(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::WorkerReset(hoimin_core::WorkerReset {
            id: reset_id,
            worker: 0,
        }),
    )
    .unwrap();
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (next, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(fixture_candidate(2)),
            next_offset: 20,
        }),
    )
    .unwrap();
    let (next, effects) = complete_mutant_started(next, &effects);

    assert_eq!(next.summary().killed, 1);
    assert_eq!(next.exit_code(), 4);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::RunMutant(_)))
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RunEffect::EmitOutput(value)
            if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(value)
                if value.status == MutationStatus::NotRun && value.candidate.sequence == 2)
    )));
    let finished_id = effect_id(find_effect(
        &effects,
        |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(_))),
    ));
    let (next, _) = transition(
        next,
        RunEvent::OutputEmitted(OutputEmitted { id: finished_id }),
    )
    .unwrap();
    assert_eq!(next.summary().not_run, 1);
}

#[test]
fn completion_ledger_stays_bounded_across_ten_thousand_mutants() {
    let mut raw = fixture_raw_config();
    raw.limits.max_mutants = 10_000;
    let (state, effects) = waiting_for_analysis_with(RunConfig::try_from(raw).unwrap());
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    let (mut state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "large".to_owned(),
                records: 10_000,
            }),
            truncated: false,
        }),
    )
    .unwrap();
    let mut read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    for sequence in 1..=10_000 {
        let (next, effects) = transition(
            state,
            RunEvent::CandidateLoaded(CandidateLoaded {
                id: read_id,
                worker: 0,
                candidate: Some(fixture_candidate(sequence)),
                next_offset: sequence,
            }),
        )
        .unwrap();
        state = next;
        let apply_id = effect_id(find_effect(&effects, |effect| {
            matches!(effect, RunEffect::ApplyMutation(_))
        }));
        let (next, effects) = transition(
            state,
            RunEvent::MutationApplied(MutationApplied {
                id: apply_id,
                worker: 0,
            }),
        )
        .unwrap();
        state = next;
        let (next, effects) = complete_mutant_started(state, &effects);
        state = next;
        let mutant_id = effect_id(find_effect(&effects, |effect| {
            matches!(effect, RunEffect::RunMutant(_))
        }));
        let (next, effects) = transition(
            state,
            RunEvent::MutantFinished(process_finished(mutant_id, ProcessTermination::Exit(1))),
        )
        .unwrap();
        state = next;
        let finished_id = effect_id(find_effect(&effects, |effect| {
            matches!(effect, RunEffect::EmitOutput(_))
        }));
        let (next, effects) = transition(
            state,
            RunEvent::OutputEmitted(OutputEmitted { id: finished_id }),
        )
        .unwrap();
        state = next;
        let reset_id = effect_id(find_effect(&effects, |effect| {
            matches!(effect, RunEffect::ResetWorker(_))
        }));
        let (next, effects) = transition(
            state,
            RunEvent::WorkerReset(hoimin_core::WorkerReset {
                id: reset_id,
                worker: 0,
            }),
        )
        .unwrap();
        state = next;
        assert!(state.completion_ledger_entries() <= state.pending_count());
        read_id = effect_id(find_effect(&effects, |effect| {
            matches!(effect, RunEffect::ReadCandidate(_))
        }));
    }
    let (state, _) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: None,
            next_offset: 10_000,
        }),
    )
    .unwrap();
    assert_eq!(state.completion_ledger_entries(), 0);
    assert_eq!(state.summary().killed, 10_000);
}

fn start_state() -> (RunState, Vec<RunEffect>) {
    transition(
        RunState::new("run-1", fixture_config()),
        RunEvent::StartRequested(StartRequested),
    )
    .unwrap()
}

fn waiting_for_baseline() -> (RunState, Vec<RunEffect>) {
    let (state, effects) = start_state();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: resolve_id,
            targets: vec![TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            }],
        }),
    )
    .unwrap();
    let preflight_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Preflight(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::PreflightCompleted(PreflightCompleted {
            id: preflight_id,
            per_worker_logical_bytes: 10,
            requested_workers: 1,
            aggregate_logical_bytes: 10,
            fingerprint: None,
        }),
    )
    .unwrap();
    let (state, effects) = complete_run_started(state, &effects);
    let create = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::CreateWorker(_))
    });
    let (state, effects) = transition(
        state,
        RunEvent::WorkerCreated(WorkerCreated {
            id: effect_id(create),
            worker: 0,
            reservation_id: reservation_id(create),
        }),
    )
    .unwrap();
    (state, effects)
}

fn waiting_for_analysis() -> (RunState, Vec<RunEffect>) {
    waiting_for_analysis_with(fixture_config())
}

fn waiting_for_analysis_with(config: RunConfig) -> (RunState, Vec<RunEffect>) {
    let (state, effects) = waiting_for_baseline_with(config);
    let baseline_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunBaseline(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(baseline_id, ProcessTermination::Exit(0))),
    )
    .unwrap();
    let output_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(_))
    }));
    let (state, emitted) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: output_id }),
    )
    .unwrap();
    assert!(emitted.is_empty());
    (state, effects)
}

fn waiting_for_baseline_with(config: RunConfig) -> (RunState, Vec<RunEffect>) {
    let session_enabled = config.session.is_some();
    let resume = config.resume;
    let jobs = u32::try_from(config.limits.jobs.get()).unwrap();
    let initial_state = if session_enabled {
        RunState::with_fingerprint("run-1", config, RunFingerprint::from_bytes([7; 32]))
    } else {
        RunState::new("run-1", config)
    };
    let (state, effects) =
        transition(initial_state, RunEvent::StartRequested(StartRequested)).unwrap();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: resolve_id,
            targets: vec![TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            }],
        }),
    )
    .unwrap();
    let preflight_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Preflight(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::PreflightCompleted(PreflightCompleted {
            id: preflight_id,
            per_worker_logical_bytes: 10,
            requested_workers: jobs,
            aggregate_logical_bytes: 10 * u64::from(jobs),
            fingerprint: session_enabled.then_some(RunFingerprint::from_bytes([7; 32])),
        }),
    )
    .unwrap();
    let (mut state, mut effects) = (state, effects);
    if session_enabled {
        if resume {
            let id = effect_id(find_effect(&effects, |effect| {
                matches!(effect, RunEffect::LoadSession(_))
            }));
            (state, effects) = transition(
                state,
                RunEvent::SessionLoaded(SessionLoaded {
                    id,
                    resume: Some(SessionResumeRef {
                        run_id: "session-run".to_owned(),
                    }),
                }),
            )
            .unwrap();
        } else {
            let id = effect_id(find_effect(&effects, |effect| {
                matches!(effect, RunEffect::BeginSession(_))
            }));
            (state, effects) = transition(
                state,
                RunEvent::SessionStarted(SessionStarted {
                    id,
                    run_id: "session-run".to_owned(),
                }),
            )
            .unwrap();
        }
    }
    (state, effects) = complete_run_started(state, &effects);
    let creates: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            RunEffect::CreateWorker(value) => {
                Some((value.id(), value.worker(), value.reservation_id()))
            }
            _ => None,
        })
        .collect();
    let mut produced = Vec::new();
    for (id, worker, reservation_id) in creates {
        (state, produced) = transition(
            state,
            RunEvent::WorkerCreated(WorkerCreated {
                id,
                worker,
                reservation_id,
            }),
        )
        .unwrap();
    }
    (state, produced)
}

fn complete_run_started(state: RunState, effects: &[RunEffect]) -> (RunState, Vec<RunEffect>) {
    let output_id = effect_id(find_effect(
        effects,
        |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::RunStarted(_))),
    ));
    transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: output_id }),
    )
    .unwrap()
}

fn complete_mutant_started(state: RunState, effects: &[RunEffect]) -> (RunState, Vec<RunEffect>) {
    let output_id = effect_id(find_effect(
        effects,
        |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::MutantStarted(_))),
    ));
    transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: output_id }),
    )
    .unwrap()
}

fn waiting_for_candidate() -> (RunState, Vec<RunEffect>) {
    let (state, effects) = waiting_for_analysis();
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "candidates".to_owned(),
                records: 1,
            }),
            truncated: false,
        }),
    )
    .unwrap()
}

fn waiting_for_filtered_analysis(candidate_ids: BTreeSet<String>) -> (RunState, Vec<RunEffect>) {
    let initial_state = RunState::with_candidate_filter("run-1", fixture_config(), candidate_ids);
    let (state, effects) =
        transition(initial_state, RunEvent::StartRequested(StartRequested)).unwrap();
    let resolve_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ResolveTargets(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: resolve_id,
            targets: vec![TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            }],
        }),
    )
    .unwrap();
    let preflight_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::Preflight(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::PreflightCompleted(PreflightCompleted {
            id: preflight_id,
            per_worker_logical_bytes: 10,
            requested_workers: 1,
            aggregate_logical_bytes: 10,
            fingerprint: None,
        }),
    )
    .unwrap();
    let (state, effects) = complete_run_started(state, &effects);
    let create = find_effect(&effects, |effect| {
        matches!(effect, RunEffect::CreateWorker(_))
    });
    let (state, effects) = transition(
        state,
        RunEvent::WorkerCreated(WorkerCreated {
            id: effect_id(create),
            worker: 0,
            reservation_id: reservation_id(create),
        }),
    )
    .unwrap();
    let baseline_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunBaseline(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(baseline_id, ProcessTermination::Exit(0))),
    )
    .unwrap();
    let output_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::EmitOutput(_))
    }));
    let (state, emitted) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: output_id }),
    )
    .unwrap();
    assert!(emitted.is_empty());
    (state, effects)
}

fn waiting_for_session_candidate(resume: bool) -> (RunState, Vec<RunEffect>) {
    let mut raw = fixture_raw_config();
    raw.session = Some(hoimin_core::SessionConfig {
        path: "session.sqlite3".into(),
    });
    raw.resume = resume;
    let (state, effects) = waiting_for_analysis_with(RunConfig::try_from(raw).unwrap());
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));
    transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "candidates".to_owned(),
                records: 1,
            }),
            truncated: false,
        }),
    )
    .unwrap()
}

fn waiting_for_reset(termination: ProcessTermination) -> (RunState, Vec<RunEffect>) {
    let (state, effects) = waiting_for_candidate();
    let read_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ReadCandidate(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read_id,
            worker: 0,
            candidate: Some(fixture_candidate(1)),
            next_offset: 1,
        }),
    )
    .unwrap();
    let apply_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::ApplyMutation(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply_id,
            worker: 0,
        }),
    )
    .unwrap();
    let (state, effects) = complete_mutant_started(state, &effects);
    let mutant_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::RunMutant(_))
    }));
    let (state, effects) = transition(
        state,
        RunEvent::MutantFinished(process_finished(mutant_id, termination)),
    )
    .unwrap();
    let finished_id = effect_id(find_effect(
        &effects,
        |effect| matches!(effect, RunEffect::EmitOutput(value) if matches!(&value.event, hoimin_core::OutputEvent::MutantFinished(_))),
    ));
    transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: finished_id }),
    )
    .unwrap()
}

fn fixture_config() -> RunConfig {
    RunConfig::try_from(fixture_raw_config()).unwrap()
}

fn focused_config() -> RunConfig {
    let mut raw = fixture_raw_config();
    raw.profile = MutationProfile::Focused;
    RunConfig::try_from(raw).unwrap()
}

fn fixture_raw_config() -> RawRunConfig {
    RawRunConfig {
        root: ".".into(),
        sources: vec!["src".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        limits: RawRunLimits {
            baseline_timeout: Duration::from_secs(1),
            ..RawRunLimits::default()
        },
        output: OutputConfig::default(),
        ..RawRunConfig::default()
    }
}

fn process_finished(id: EffectId, termination: ProcessTermination) -> ProcessFinished {
    ProcessFinished {
        id,
        worker: Some(0),
        termination,
        output: hoimin_core::OutputSpoolRef {
            token: "output".to_owned(),
            retained: 0,
            observed: 0,
        },
        elapsed: Duration::from_millis(5),
        resource_mode: ResourceMode::Hard,
    }
}

fn fixture_candidate(sequence: u64) -> MutationCandidate {
    MutationCandidate {
        id: format!("m{sequence}"),
        sequence,
        path: "src/calc.py".into(),
        span: ByteSpan {
            start: 1,
            length: 1,
        },
        original: "+".to_owned(),
        replacement: "-".to_owned(),
        operator: "binary_add_sub".to_owned(),
        line: 1,
        column: 1,
        symbol: None,
        file_hash: "hash".to_owned(),
    }
}

fn find_effect(effects: &[RunEffect], predicate: impl Fn(&RunEffect) -> bool) -> &RunEffect {
    effects.iter().find(|effect| predicate(effect)).unwrap()
}

fn effect_id(effect: &RunEffect) -> EffectId {
    effect.id()
}

fn reservation_id(effect: &RunEffect) -> hoimin_core::ReservationId {
    match effect {
        RunEffect::CreateWorker(value) => value.reservation_id(),
        _ => panic!("expected create worker"),
    }
}
