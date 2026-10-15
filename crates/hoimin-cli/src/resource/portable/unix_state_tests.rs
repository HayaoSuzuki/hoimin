//! Characterization through the Unix syscall boundary: all supervisor transitions
//! (including Drop) remain real. No test identifier is sent to the OS.
use std::cell::RefCell;
use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use proptest::prelude::*;

use super::PortableSupervisor;

#[derive(Default)]
struct FakeOs {
    calls: Vec<(i32, i32)>,
    probe_errno: Option<i32>,
    signal_errno: Option<i32>,
}

thread_local! {
    static OS: RefCell<Option<FakeOs>> = const { RefCell::new(None) };
}

pub(super) fn intercept_kill(group: i32, signal: i32) -> Option<io::Result<()>> {
    OS.with_borrow_mut(|os| {
        let os = os.as_mut()?;
        os.calls.push((group, signal));
        let errno = if signal == 0 {
            os.probe_errno
        } else {
            os.signal_errno
        };
        Some(errno.map_or(Ok(()), |errno| Err(io::Error::from_raw_os_error(errno))))
    })
}

struct OsGuard;

#[allow(
    clippy::unused_self,
    reason = "fake OS access requires the RAII guard to remain installed"
)]
impl OsGuard {
    fn install() -> Self {
        OS.with_borrow_mut(|os| {
            assert!(os.is_none(), "nested fake OS");
            *os = Some(FakeOs::default());
        });
        Self
    }

    fn probe_errno(&self, errno: Option<i32>) {
        OS.with_borrow_mut(|os| os.as_mut().unwrap().probe_errno = errno);
    }

    fn signal_errno(&self, errno: Option<i32>) {
        OS.with_borrow_mut(|os| os.as_mut().unwrap().signal_errno = errno);
    }

    fn calls(&self) -> Vec<(i32, i32)> {
        OS.with_borrow(|os| os.as_ref().unwrap().calls.clone())
    }
}

impl Drop for OsGuard {
    fn drop(&mut self) {
        OS.with_borrow_mut(|os| *os = None);
    }
}

fn supervisor() -> PortableSupervisor {
    OS.with_borrow(|os| assert!(os.is_some(), "synthetic PGID requires a fake OS"));
    let mut supervisor = PortableSupervisor::new(Arc::default(), Arc::default()).unwrap();
    supervisor.process_group = super::ProcessGroup::Signalable(41);
    supervisor
}

#[test]
fn repeated_reap_prevents_drop_from_signaling() {
    // Arrange: the guard is declared first and outlives supervisor even on panic.
    let os = OsGuard::install();
    let mut supervisor = supervisor();

    // Act
    supervisor.record_root_reaped();
    supervisor.record_root_reaped();
    drop(supervisor);

    // Assert: a reap must remove termination authority, even on repeated notification.
    assert_eq!(os.calls(), [] as [(i32, i32); 0]);
}

#[test]
fn probe_error_retry_and_drop_never_signal_reaped_group() {
    // Arrange
    let os = OsGuard::install();
    os.probe_errno(Some(libc::EIO));
    let mut supervisor = supervisor();
    supervisor.record_root_reaped();

    // Act
    let first = supervisor.terminate(false);
    os.probe_errno(None);
    let retry = supervisor.terminate(false).unwrap();
    drop(supervisor);

    // Assert: an error must retain the probe ID without restoring signal authority.
    assert!(first.is_err());
    assert!(!retry);
    assert_eq!(os.calls(), [(41, 0), (41, 0)]);
}

#[test]
fn implicit_reap_disarms_before_probe_error_and_drop() {
    // Arrange
    let os = OsGuard::install();
    os.probe_errno(Some(libc::EIO));
    let mut supervisor = supervisor();

    // Act
    let result = supervisor.terminate(false);
    drop(supervisor);

    // Assert
    assert!(result.is_err());
    assert_eq!(os.calls(), [(41, 0)]);
}

#[test]
fn termination_failure_after_reap_cannot_rearm_drop() {
    // Arrange
    let os = OsGuard::install();
    let mut supervisor = supervisor();
    supervisor.termination_failures.store(1, Ordering::Release);

    // Act
    let result = supervisor.terminate(false);
    drop(supervisor);

    // Assert
    assert!(result.is_err());
    assert_eq!(os.calls(), [(41, 0)]);
}

#[test]
fn absence_evidence_survives_repeated_refresh_without_an_identifier() {
    // Arrange
    let os = OsGuard::install();
    os.probe_errno(Some(libc::ESRCH));
    let mut supervisor = supervisor();

    // Act
    let first = supervisor
        .refresh_tree_quiescence_after_root_reap()
        .unwrap();
    let second = supervisor
        .refresh_tree_quiescence_after_root_reap()
        .unwrap();
    drop(supervisor);

    // Assert: clearing the ID must not erase already established quiescence.
    assert!(first);
    assert!(second);
    assert_eq!(os.calls(), [(41, 0)]);
}

#[test]
fn unattached_supervisor_does_not_claim_absence() {
    // Arrange
    let os = OsGuard::install();
    let mut supervisor = PortableSupervisor::new(Arc::default(), Arc::default()).unwrap();

    // Act
    let result = supervisor.terminate(false).unwrap();
    drop(supervisor);

    // Assert
    assert!(!result);
    assert_eq!(os.calls(), [] as [(i32, i32); 0]);
}

#[test]
fn successful_signal_requires_later_absence_evidence() {
    // Arrange
    let os = OsGuard::install();
    let mut supervisor = supervisor();

    // Act
    let terminated = supervisor.terminate(true).unwrap();
    supervisor.record_root_reaped();
    let cached = supervisor.terminate(false).unwrap();
    os.probe_errno(Some(libc::ESRCH));
    let refreshed = supervisor
        .refresh_tree_quiescence_after_root_reap()
        .unwrap();
    drop(supervisor);

    // Assert: termination success and quiescence are independent facts.
    assert!(!terminated);
    assert!(!cached);
    assert!(refreshed);
    assert_eq!(os.calls(), [(41, libc::SIGKILL), (41, 0)]);
}

#[test]
fn probe_errno_boundaries_preserve_retry_identity() {
    for (errno, expected) in [
        (None, Ok(false)),
        (Some(libc::EPERM), Ok(false)),
        (Some(libc::ESRCH), Ok(true)),
        (Some(libc::EIO), Err(())),
    ] {
        // Arrange
        let os = OsGuard::install();
        os.probe_errno(errno);
        let mut supervisor = supervisor();

        // Act
        let actual = supervisor
            .refresh_tree_quiescence_after_root_reap()
            .map_err(|_| ());
        os.probe_errno(Some(libc::ESRCH));
        let retry = supervisor
            .refresh_tree_quiescence_after_root_reap()
            .unwrap();
        drop(supervisor);

        // Assert: only ESRCH proves absence; other outcomes retain the probe target.
        assert_eq!(actual, expected, "errno={errno:?}");
        assert!(retry);
        let expected_calls = if errno == Some(libc::ESRCH) {
            vec![(41, 0)]
        } else {
            vec![(41, 0), (41, 0)]
        };
        assert_eq!(os.calls(), expected_calls, "errno={errno:?}");
    }
}

#[test]
fn signal_errno_boundaries_preserve_retry_behavior() {
    for (errno, expected, calls) in [
        (None, Ok(false), 1),
        (Some(libc::ESRCH), Ok(false), 1),
        (Some(libc::EPERM), Err(()), 2),
        (Some(libc::EIO), Err(()), 2),
    ] {
        // Arrange
        let os = OsGuard::install();
        os.signal_errno(errno);
        let mut supervisor = supervisor();

        // Act
        let actual = supervisor.terminate(true).map_err(|_| ());
        os.signal_errno(None);
        let retry = supervisor.terminate(true).unwrap();
        drop(supervisor);

        // Assert: ESRCH is successful termination, while other errors allow retry.
        assert_eq!(actual, expected, "errno={errno:?}");
        assert!(!retry);
        assert_eq!(os.calls(), vec![(41, libc::SIGKILL); calls]);
    }
}

proptest! {
    #[test]
    fn operation_history_preserves_signal_authority(
        operations in prop::collection::vec((0u8..4, 0u8..3), 0..80)
    ) {
        // Arrange: the oracle records independent lifecycle facts, not enum variants.
        let os = OsGuard::install();
        let mut supervisor = supervisor();
        let mut reap_observed = false;
        let mut absence_observed = false;
        let mut termination_completed = false;
        let mut quiescent = false;
        let mut expected_calls = Vec::new();

        // Act and assert each prefix so later operations cannot conceal a wrong step.
        for (operation, outcome) in operations {
            os.probe_errno(match outcome { 0 => None, 1 => Some(libc::ESRCH), _ => Some(libc::EIO) });
            if operation == 0 {
                supervisor.record_root_reaped();
                reap_observed = true;
                continue;
            }
            let live = operation == 1;
            let refresh = operation == 3;
            let expected = if (refresh && quiescent) || (!refresh && termination_completed) {
                Ok(quiescent)
            } else {
                let mut probe_error = false;
                let mut observed_now = false;
                if refresh || !live {
                    reap_observed = true;
                    if !absence_observed {
                        expected_calls.push((41, 0));
                        probe_error = outcome == 2;
                        observed_now = outcome == 1;
                        absence_observed |= observed_now;
                    }
                } else if !reap_observed {
                    expected_calls.push((41, libc::SIGKILL));
                }
                if probe_error { Err(()) } else {
                    quiescent = observed_now;
                    termination_completed |= !refresh;
                    Ok(quiescent)
                }
            };
            let actual = if refresh {
                supervisor.refresh_tree_quiescence_after_root_reap()
            } else { supervisor.terminate(live) }.map_err(|_| ());
            prop_assert_eq!(actual, expected);
            prop_assert_eq!(os.calls(), expected_calls.clone());
        }
        if !termination_completed && !reap_observed {
            expected_calls.push((41, libc::SIGKILL));
        }
        drop(supervisor);
        prop_assert_eq!(os.calls(), expected_calls);
    }
}
