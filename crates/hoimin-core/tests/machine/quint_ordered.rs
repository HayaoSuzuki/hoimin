//! Replay bounded discovery and indexed reads through the public machine API.
use super::{fixture_candidate, quint, waiting_for_ordered_analysis_with_jobs};
use hoimin_core::{
    AnalysisFinished, CandidateLoaded, CandidateReadTarget, CandidateSpoolRef, MachineError,
    RunEffect, RunEvent, RunState, transition,
};
use serde_json::Value;

fn read_target(value: &Value) -> Option<CandidateReadTarget> {
    match value["tag"].as_str().unwrap() {
        "Scan" => Some(CandidateReadTarget::Next),
        "Fetch" => Some(CandidateReadTarget::Id(format!(
            "m{}",
            quint::number(&value["value"])
        ))),
        "NoRead" => None,
        other => panic!("unknown read target {other}"),
    }
}

fn bootstrap() -> (RunState, Vec<RunEffect>) {
    let (state, pending) =
        waiting_for_ordered_analysis_with_jobs(vec!["m3".into(), "m1".into()], 2);
    let [RunEffect::AnalyzeFile(request)] = pending.as_slice() else {
        panic!("expected analysis request, got {pending:?}");
    };
    transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: request.id,
            spool: Some(CandidateSpoolRef {
                token: "ordered".into(),
                records: 4,
            }),
            truncated: false,
            diagnostics: Vec::new(),
        }),
    )
    .unwrap()
}

fn check_pending(state: &RunState, pending: &[RunEffect], expected: &Value) {
    let mut reads = Vec::new();
    let mut actual_ids = Vec::new();
    for effect in pending {
        assert!(state.is_effect_pending(effect.id()));
        match effect {
            RunEffect::ReadCandidate(request) => reads.push(&request.target),
            RunEffect::ApplyMutation(request) => actual_ids.push(&request.candidate.id),
            other => panic!("unexpected effect before worker handoff: {other:?}"),
        }
    }
    assert_eq!(state.pending_count(), pending.len());
    assert_eq!(
        reads,
        read_target(&expected["read"]).iter().collect::<Vec<_>>()
    );
    let model_order: Vec<_> = expected["applied"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| format!("m{}", quint::number(id)))
        .collect();
    assert_eq!(actual_ids, model_order.iter().collect::<Vec<_>>());
}

fn replay(history: &[Value]) {
    let (mut state, mut pending) = bootstrap();
    check_pending(&state, &pending, &history[0]["s"]);
    for (step, row) in history.iter().enumerate().skip(1) {
        let expected = &row["s"];
        let command = &row["dispatch"];
        let read = pending
            .iter()
            .find_map(|effect| match effect {
                RunEffect::ReadCandidate(request) => Some(request),
                _ => None,
            })
            .expect("pending model read");
        let id = match command["tag"].as_str().unwrap() {
            "Discover" | "Body" => quint::number(&command["value"]),
            "EndOfSpool" => 0,
            other => panic!("unknown discovery input {other}"),
        };
        let event = RunEvent::CandidateLoaded(CandidateLoaded {
            id: read.id,
            worker: read.worker,
            candidate: (id != 0).then(|| fixture_candidate(id)),
        });
        let missing = quint::number(&expected["missing"]);
        if missing != 0 {
            assert_eq!(
                transition(state, event).unwrap_err(),
                MachineError::SelectedCandidateMissing(format!("m{missing}"))
            );
            assert_eq!(
                step + 1,
                history.len(),
                "missing selection terminates the model"
            );
            return;
        }
        if expected["mismatch"] == true {
            let Some(CandidateReadTarget::Id(wanted)) = read_target(&expected["read"]) else {
                panic!("mismatched body must have an indexed target");
            };
            assert_eq!(
                transition(state.clone(), event).unwrap_err(),
                MachineError::CandidateIdentityMismatch {
                    worker: read.worker,
                    expected: wanted,
                    received: (id != 0).then(|| format!("m{id}")),
                },
            );
        } else {
            let read_id = read.id;
            let (next, effects) = transition(state, event).unwrap();
            state = next;
            pending.retain(|effect| effect.id() != read_id);
            pending.extend(effects);
        }
        check_pending(&state, &pending, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/ordered_discovery_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/ordered_discovery_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/ordered_discovery_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/ordered_discovery_3.itf.json"))]
#[case::ordered(include_str!("../../../../formal/quint/traces/ordered_discovery_orderedStep_0.itf.json"))]
#[case::missing(include_str!("../../../../formal/quint/traces/ordered_discovery_missingStep_0.itf.json"))]
fn discovery_history_matches_public_requests(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
