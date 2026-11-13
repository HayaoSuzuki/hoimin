//! Scoped one-shot failures for shell-owned creation operations in tests.
use crate::workspace::WorkspaceError;
use camino::Utf8Path;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Stage {
    ExecutionRoot,
    DeliveryRoot,
    ExecutionSpool,
    DeliverySpool,
}

thread_local! {
    static ACTIVE: Cell<Option<Stage>> = const { Cell::new(None) };
}

pub(super) struct Guard(Option<Stage>);

impl Guard {
    pub(super) fn arm(stage: Stage) -> Self {
        Self(ACTIVE.with(|active| active.replace(Some(stage))))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(self.0));
    }
}

pub(super) fn perform<T>(
    stage: Stage,
    parent: &Utf8Path,
    operation: impl FnOnce() -> Result<T, WorkspaceError>,
) -> Result<T, WorkspaceError> {
    let fail = ACTIVE.with(|active| {
        if active.get() == Some(stage) {
            active.set(None);
            true
        } else {
            false
        }
    });
    if fail {
        return Err(WorkspaceError::io(
            "create shell owner",
            parent,
            std::io::Error::other(format!("injected shell owner operation: {stage:?}")),
        ));
    }
    operation()
}
