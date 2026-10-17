use hoimin_core::{ProcessLimits, ProcessTermination};
use proptest::prelude::*;
use tokio::process::Command;
use uuid::Uuid;
use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;
use windows_sys::Win32::System::SystemServices::{
    JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS, JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT,
    JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO, JOB_OBJECT_MSG_EXIT_PROCESS,
    JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
};

use super::{ActiveRoot, RunState, WindowsBackend, detach_root_generation, record_notification};
use crate::resource::ProcessSupervisor;

fn root(id: u128, pid: u32, interested: bool) -> ActiveRoot {
    ActiveRoot {
        id: Uuid::from_u128(id),
        pid,
        record_exit: interested,
        process: None,
    }
}

#[test]
fn exit_notification_records_only_interested_roots() {
    for (message, interested, expected) in [
        (JOB_OBJECT_MSG_EXIT_PROCESS, false, vec![]),
        (JOB_OBJECT_MSG_EXIT_PROCESS, true, vec![Uuid::from_u128(1)]),
        (JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS, false, vec![]),
        (
            JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS,
            true,
            vec![Uuid::from_u128(1)],
        ),
    ] {
        let mut state = RunState {
            active: vec![root(1, 101, interested), root(2, 102, true)],
            ..RunState::default()
        };

        record_notification(&mut state, message, 101, false);

        assert_eq!(state.exited_roots, expected.into_iter().collect());
        assert_eq!(
            state.active.iter().map(|root| root.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(2)]
        );
    }
}

#[test]
fn active_process_zero_requires_empty_job_and_respects_interest() {
    for (interested, job_is_empty, expected_active, expected_exited) in [
        (false, false, vec![Uuid::from_u128(1)], vec![]),
        (true, false, vec![Uuid::from_u128(1)], vec![]),
        (false, true, vec![], vec![]),
        (true, true, vec![], vec![Uuid::from_u128(1)]),
    ] {
        let mut state = RunState {
            active: vec![root(1, 101, interested)],
            ..RunState::default()
        };

        record_notification(
            &mut state,
            JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO,
            0,
            job_is_empty,
        );

        assert_eq!(
            state.active.iter().map(|root| root.id).collect::<Vec<_>>(),
            expected_active
        );
        assert_eq!(state.exited_roots, expected_exited.into_iter().collect());
    }
}

#[test]
fn run_wide_limit_notifications_preserve_root_registry() {
    for message in [
        JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
        JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT,
    ] {
        let mut state = RunState {
            active: vec![root(1, 101, true), root(2, 102, false)],
            exited_roots: [Uuid::from_u128(3)].into_iter().collect(),
            ..RunState::default()
        };

        record_notification(&mut state, message, 101, false);

        assert_eq!(
            state
                .active
                .iter()
                .map(|root| (root.id, root.pid, root.record_exit))
                .collect::<Vec<_>>(),
            vec![
                (Uuid::from_u128(1), 101, true),
                (Uuid::from_u128(2), 102, false)
            ]
        );
        assert_eq!(
            state.exited_roots,
            [Uuid::from_u128(3)].into_iter().collect()
        );
    }
}

#[test]
fn nested_limit_notifications_choose_memory_before_process_limit() {
    for (messages, input, expected) in [
        (
            vec![],
            ProcessTermination::Exit(7),
            ProcessTermination::Exit(7),
        ),
        (
            vec![],
            ProcessTermination::Timeout,
            ProcessTermination::Timeout,
        ),
        (
            vec![JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT],
            ProcessTermination::Exit(7),
            ProcessTermination::ProcessLimit,
        ),
        (
            vec![JOB_OBJECT_MSG_JOB_MEMORY_LIMIT],
            ProcessTermination::Exit(7),
            ProcessTermination::OutOfMemory,
        ),
        (
            vec![
                JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT,
                JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
            ],
            ProcessTermination::Exit(7),
            ProcessTermination::OutOfMemory,
        ),
        (
            vec![
                JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
                JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT,
            ],
            ProcessTermination::Exit(7),
            ProcessTermination::OutOfMemory,
        ),
    ] {
        let backend = WindowsBackend::new().unwrap();
        let mut command = Command::new("cmd.exe");
        let limits = ProcessLimits {
            timeout: std::time::Duration::from_secs(1),
            max_memory_bytes: 256 * 1024 * 1024,
            max_processes: 8,
            max_output_bytes: 64,
        };
        let ProcessSupervisor::Windows(supervisor) = backend.prepare(&mut command, limits).unwrap()
        else {
            panic!("Windows supervisor required");
        };
        for message in messages {
            // SAFETY: the supervisor owns this live port; a null PID is sufficient for limit messages.
            assert_ne!(
                unsafe {
                    PostQueuedCompletionStatus(
                        supervisor.completion_port.raw(),
                        message,
                        0,
                        std::ptr::null(),
                    )
                },
                0
            );
        }

        let actual = supervisor.classify_root_notification(input);

        assert_eq!(actual, expected);
    }
}

proptest! {
    #[test]
    fn delayed_exit_sequences_preserve_pid_generation_order(
        messages in prop::collection::vec(prop::sample::select(vec![
            JOB_OBJECT_MSG_EXIT_PROCESS,
            JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS,
            JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO,
            JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
            JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT,
        ]), 0..32),
    ) {
        let mut state = RunState {
            active: vec![root(1, 101, true), root(2, 101, true), root(3, 102, true)],
            ..RunState::default()
        };
        let exit_count = messages.iter().filter(|&&message|
            message == JOB_OBJECT_MSG_EXIT_PROCESS || message == JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS
        ).count();
        let expected_active = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)]
            .into_iter().skip(exit_count.min(2)).collect::<Vec<_>>();
        let expected_exited = [Uuid::from_u128(2)].into_iter()
            .take(usize::from(exit_count >= 2)).collect();

        detach_root_generation(&mut state, Uuid::from_u128(1));
        for message in messages {
            record_notification(&mut state, message, 101, false);
        }

        prop_assert_eq!(state.active.iter().map(|root| root.id).collect::<Vec<_>>(), expected_active);
        prop_assert_eq!(state.exited_roots, expected_exited);
    }
}
