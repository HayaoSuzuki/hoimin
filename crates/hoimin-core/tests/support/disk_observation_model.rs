//! Evidence projection and public API calls only; expectations belong to Quint.
use hoimin_core::disk::{
    DiskFailureReason, DiskLifecycle, DiskLifecycleEvent, DiskObservation, DiskPolicy,
    DiskSecondary, DiskStopReason,
};
use serde::Deserialize;
use serde_json::Value;
use std::{num::NonZeroU64, time::Duration};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Default)]
pub struct Evidence {
    reason: u64,
    owned: u64,
    free: u64,
    elapsed: u64,
    message: u64,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    primary: Evidence,
    secondary: Vec<Evidence>,
    dispatched: u64,
    accepted: bool,
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Observe(u64, u64, u64),
    Fail(u64),
    Dispatch,
    Idle,
}
pub struct Driver {
    inner: DiskLifecycle,
    accepted: bool,
}
impl Default for Driver {
    fn default() -> Self {
        Self {
            inner: DiskLifecycle::new([]).unwrap(),
            accepted: true,
        }
    }
}
impl Driver {
    pub fn apply(&mut self, command: Command) {
        let before = self.snapshot();
        let event = match command {
            Command::Observe(owned, free, elapsed) => DiskLifecycleEvent::Observation {
                policy: DiskPolicy {
                    max_owned_bytes: NonZeroU64::new(2).unwrap(),
                    min_free_bytes: NonZeroU64::new(1).unwrap(),
                },
                value: DiskObservation {
                    owned_bytes: owned,
                    available_bytes: free,
                    measured_in: Duration::from_millis(elapsed),
                },
            },
            Command::Fail(message) => DiskLifecycleEvent::MeasurementFailed {
                message: message.to_string(),
            },
            Command::Dispatch => DiskLifecycleEvent::DispatchRequested,
            Command::Idle => return,
        };
        self.accepted = self.inner.apply(event);
        let after = self.snapshot();
        if before.primary.reason != 0 {
            assert_eq!(
                after.primary, before.primary,
                "first failure evidence is immutable"
            );
            assert_eq!(
                after.dispatched, before.dispatched,
                "stopped dispatch is rejected"
            );
        }
        assert!(after.evidence_is_unique());
    }
    pub fn snapshot(&self) -> Snapshot {
        let snapshot = self.inner.snapshot();
        let primary =
            snapshot
                .stop
                .as_ref()
                .map_or_else(Evidence::default, |failure| match &failure.reason {
                    DiskFailureReason::WorkspaceSizeExceeded(value) => observed(1, value),
                    DiskFailureReason::FilesystemReserveReached(value) => observed(2, value),
                    DiskFailureReason::MeasurementFailed(message) => Evidence {
                        reason: 3,
                        message: message.parse().unwrap(),
                        ..Evidence::default()
                    },
                    other @ DiskFailureReason::ProcessFailed(_) => {
                        panic!("unexpected primary {other:?}")
                    }
                });
        assert!(
            snapshot
                .stop
                .as_ref()
                .is_none_or(|failure| failure.secondary == snapshot.secondary)
        );
        let secondary = snapshot
            .secondary
            .iter()
            .map(|evidence| match evidence {
                DiskSecondary::Observation { reason, value } => observed(
                    match reason {
                        DiskStopReason::WorkspaceSizeExceeded => 1,
                        DiskStopReason::FilesystemReserveReached => 2,
                        other => panic!("unexpected observation {other:?}"),
                    },
                    value,
                ),
                DiskSecondary::Error { code, message } => {
                    assert_eq!(code, hoimin_core::disk::DISK_MEASUREMENT_FAILED);
                    Evidence {
                        reason: 3,
                        message: message.parse().unwrap(),
                        ..Evidence::default()
                    }
                }
            })
            .collect();
        Snapshot {
            primary,
            secondary,
            dispatched: snapshot.dispatched,
            accepted: self.accepted,
        }
    }
}
fn observed(reason: u64, value: &DiskObservation) -> Evidence {
    Evidence {
        reason,
        owned: value.owned_bytes,
        free: value.available_bytes,
        elapsed: u64::try_from(value.measured_in.as_millis()).unwrap(),
        message: 0,
    }
}
/// Accept raw ITF integers and decimal strings emitted by `itf::Value`'s serde visitor.
pub fn number(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|v| v.parse().ok()))
        .or_else(|| value["#bigint"].as_str().and_then(|v| v.parse().ok()))
        .expect("unsigned ITF integer")
}
impl Evidence {
    fn from_spec(value: &Value) -> Self {
        Self {
            reason: number(&value["reason"]),
            owned: number(&value["owned"]),
            free: number(&value["free"]),
            elapsed: number(&value["elapsed"]),
            message: number(&value["message"]),
        }
    }
}
impl Snapshot {
    pub fn evidence_is_unique(&self) -> bool {
        self.secondary.iter().enumerate().all(|(index, entry)| {
            entry != &self.primary && !self.secondary[..index].contains(entry)
        })
    }
    pub fn from_spec(value: &Value) -> Self {
        Self {
            primary: Evidence::from_spec(&value["primary"]),
            secondary: value["secondary"]
                .as_array()
                .unwrap()
                .iter()
                .map(Evidence::from_spec)
                .collect(),
            dispatched: number(&value["dispatched"]),
            accepted: value["accepted"].as_bool().unwrap(),
        }
    }
}
pub fn command(value: &Value) -> Command {
    let data = &value["value"];
    match value["tag"].as_str().unwrap() {
        "Init" | "Idle" => Command::Idle,
        "Observe" => Command::Observe(
            number(&data["owned"]),
            number(&data["free"]),
            number(&data["elapsed"]),
        ),
        "Fail" => Command::Fail(number(&data["message"])),
        "Dispatch" => Command::Dispatch,
        other => panic!("unexpected command {other}"),
    }
}

/// One tag and three complete u64 values per command; ignore incomplete tails.
pub fn decode(bytes: &[u8]) -> Vec<Command> {
    bytes
        .as_chunks::<25>()
        .0
        .iter()
        .take(128)
        .map(|row| {
            let owned = u64::from_le_bytes(row[1..9].try_into().unwrap());
            let free = u64::from_le_bytes(row[9..17].try_into().unwrap());
            let elapsed = u64::from_le_bytes(row[17..25].try_into().unwrap());
            match row[0] % 3 {
                0 => Command::Observe(owned, free, elapsed),
                1 => Command::Fail(owned),
                _ => Command::Dispatch,
            }
        })
        .collect()
}
