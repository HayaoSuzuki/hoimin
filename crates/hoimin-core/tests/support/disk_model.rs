//! Public lifecycle commands and trajectory laws, shared with libFuzzer.
use hoimin_core::disk::{
    DiskCleanupOutcome, DiskCleanupState, DiskComponentState, DiskLifecycle, DiskLifecycleEvent,
    DiskLifecycleSnapshot, DiskObservation, DiskPolicy, DiskRootId,
};
use std::{num::NonZeroU64, time::Duration};
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Dispatch,
    Stop,
    Component(u8, bool),
    Request(u8),
    Cleanup(u8, u8),
    Finish,
    Observe(u64, u64),
}
pub fn root(index: u8) -> DiskRootId {
    match index {
        0 => DiskRootId::Execution,
        1 => DiskRootId::Delivery,
        _ => panic!("unknown root"),
    }
}
pub fn roots(mask: u8) -> Vec<DiskRootId> {
    (0..2).filter(|i| mask & (1 << i) != 0).map(root).collect()
}
pub fn event(command: Command) -> DiskLifecycleEvent {
    use DiskLifecycleEvent as E;
    match command {
        Command::Dispatch => E::DispatchRequested,
        Command::Stop => E::MeasurementFailed {
            message: "model".into(),
        },
        Command::Component(index, success) => match (index, success) {
            (0, true) => E::ProcessDrainSucceeded,
            (0, false) => E::ProcessDrainFailed,
            (1, true) => E::OutputDrainSucceeded,
            (1, false) => E::OutputDrainFailed,
            (2, true) => E::MonitorJoinSucceeded,
            (2, false) => E::MonitorJoinFailed,
            (3, true) => E::ReportSucceeded,
            (3, false) => E::ReportFailed,
            _ => panic!("unknown component"),
        },
        Command::Request(index) => E::CleanupRequested { root: root(index) },
        Command::Cleanup(index, outcome) => E::CleanupCompleted {
            root: root(index),
            outcome: match outcome {
                2 => DiskCleanupOutcome::Clean,
                3 => DiskCleanupOutcome::Failed("model".into()),
                4 => DiskCleanupOutcome::Deferred("model".into()),
                5 => DiskCleanupOutcome::Retained,
                _ => panic!("unknown cleanup"),
            },
        },
        Command::Finish => E::FinishRequested,
        Command::Observe(owned, available) => E::Observation {
            policy: DiskPolicy {
                max_owned_bytes: NonZeroU64::new(2).unwrap(),
                min_free_bytes: NonZeroU64::new(1).unwrap(),
            },
            value: DiskObservation {
                owned_bytes: owned,
                available_bytes: available,
                measured_in: Duration::ZERO,
            },
        },
    }
}
pub fn component(state: DiskComponentState) -> u64 {
    match state {
        DiskComponentState::Pending => 0,
        DiskComponentState::Succeeded => 1,
        DiskComponentState::Failed => 2,
    }
}
pub fn cleanup(snapshot: &DiskLifecycleSnapshot, index: u8) -> u64 {
    match snapshot
        .roots
        .iter()
        .find(|entry| entry.root == root(index))
        .map(|entry| &entry.cleanup)
    {
        None => 6,
        Some(DiskCleanupState::NotRequested) => 0,
        Some(DiskCleanupState::Requested) => 1,
        Some(DiskCleanupState::Completed(outcome)) => match outcome {
            DiskCleanupOutcome::Clean => 2,
            DiskCleanupOutcome::Failed(_) => 3,
            DiskCleanupOutcome::Deferred(_) => 4,
            DiskCleanupOutcome::Retained => 5,
        },
    }
}
/// Rejected operations preserve the entire snapshot, including diagnostics.
/// Successful cleanup/finish operations must carry all required prior evidence.
pub fn check_history(mask: u8, commands: &[Command]) -> Vec<bool> {
    let registered = roots(mask);
    let mut state = DiskLifecycle::new(registered.clone()).unwrap();
    let mut outcomes = Vec::new();
    for command in commands {
        let before = state.snapshot();
        let accepted = state.apply(event(*command));
        let after = state.snapshot();
        if !accepted {
            assert_eq!(after, before, "{command:?}");
        }
        if before.finished {
            assert!(!accepted);
        }
        assert!(after.dispatched >= before.dispatched);
        assert!(after.active <= after.dispatched);
        assert_eq!(
            after.roots.iter().map(|r| r.root).collect::<Vec<_>>(),
            registered
        );
        if let Some(primary) = &before.stop {
            assert_eq!(&after.stop.as_ref().unwrap().reason, &primary.reason);
        }
        for (i, value) in after.secondary.iter().enumerate() {
            assert!(!after.secondary[..i].contains(value));
        }
        let safety = [after.process_drain, after.output_drain, after.monitor_join]
            .into_iter()
            .all(|state| component(state) == 1);
        assert_eq!(state.safety_succeeded(), safety);
        if let Command::Dispatch = command {
            assert_eq!(
                accepted,
                !before.finished
                    && before.stop.is_none()
                    && before.process_drain == DiskComponentState::Pending
            );
        }
        if let Command::Cleanup(_, 2 | 3) = command
            && accepted
        {
            assert!(safety);
        }
        if state.may_finish() {
            assert_eq!(after.active, 0);
            assert_ne!(after.process_drain, DiskComponentState::Pending);
            assert_ne!(after.output_drain, DiskComponentState::Pending);
            assert_ne!(after.monitor_join, DiskComponentState::Pending);
            assert_eq!(after.report, DiskComponentState::Succeeded);
            assert!([2, 3, 5, 6].contains(&cleanup(&after, 0)));
            assert!([2, 6].contains(&cleanup(&after, 1)));
        }
        outcomes.push(accepted);
    }
    outcomes
}
pub fn decode(bytes: &[u8]) -> Vec<Command> {
    bytes
        .as_chunks::<17>()
        .0
        .iter()
        .take(128)
        .map(|row| {
            let left = u64::from_le_bytes(row[1..9].try_into().unwrap());
            let right = u64::from_le_bytes(row[9..17].try_into().unwrap());
            match row[0] % 7 {
                0 => Command::Dispatch,
                1 => Command::Stop,
                2 => Command::Component(u8::try_from(left % 4).unwrap(), right % 2 == 0),
                3 => Command::Request(u8::try_from(left % 2).unwrap()),
                4 => Command::Cleanup(
                    u8::try_from(left % 2).unwrap(),
                    u8::try_from(2 + right % 4).unwrap(),
                ),
                5 => Command::Finish,
                _ => Command::Observe(left, right),
            }
        })
        .collect()
}
