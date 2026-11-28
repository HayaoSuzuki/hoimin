use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::{
    CgroupEventCounters, ResourceError, parse_cgroup_event_counters, run_cleanup_after_accounting,
    violations_since,
};

#[derive(Debug)]
pub(super) struct LinuxRunCgroup {
    pub(super) path: PathBuf,
    #[cfg(target_os = "linux")]
    pub(super) diagnostics: Vec<String>,
    pub(super) state: Mutex<RunState>,
    pub(super) close_gate: Mutex<()>,
}

#[derive(Debug, Default)]
pub(super) struct RunState {
    pub(super) closed: bool,
    pub(super) cleaned: bool,
    pub(super) roots: HashMap<Uuid, RootEntry>,
}

#[derive(Debug)]
pub(super) struct RootEntry {
    pub(super) root: Arc<RootCgroup>,
    pub(super) signal: Arc<RootSignal>,
    pub(super) active: bool,
    pub(super) counters: CgroupEventCounters,
}

#[derive(Debug)]
pub(super) struct RootCgroup {
    pub(super) id: Uuid,
    pub(super) path: PathBuf,
    pub(super) cleanup_gate: Mutex<()>,
}

#[derive(Debug, Default)]
pub(super) struct RootSignal {
    pub(super) violations: AtomicU8,
}

impl LinuxRunCgroup {
    pub(super) fn prepare_root_with(
        &self,
        max_processes: u32,
        validate_limit: impl FnOnce() -> Result<(), ResourceError>,
        create_root: impl FnOnce(&Path) -> Result<PathBuf, ResourceError>,
        write_memory: impl FnOnce(&Path) -> Result<(), ResourceError>,
        mut write_limit: impl FnMut(&Path, &[u8]) -> Result<(), ResourceError>,
    ) -> Result<(Arc<RootCgroup>, Arc<RootSignal>), ResourceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Err(ResourceError::RunClosed);
        }
        // Reject invalid limits before creating a root cgroup or launching a child.
        validate_limit()?;
        Self::refresh_events(&mut state)?;
        let path = create_root(&self.path)?;
        write_memory(&path.join("memory.max"))?;
        write_limit(&path.join("pids.max"), max_processes.to_string().as_bytes())?;
        write_limit(&path.join("memory.oom.group"), b"1")?;
        let counters = read_events(&path)?;
        let root = Arc::new(RootCgroup {
            id: Uuid::new_v4(),
            path,
            cleanup_gate: Mutex::new(()),
        });
        let signal = Arc::new(RootSignal::default());
        state.roots.insert(
            root.id,
            RootEntry {
                root: Arc::clone(&root),
                signal: Arc::clone(&signal),
                active: false,
                counters,
            },
        );
        Ok((root, signal))
    }

    pub(super) fn attach_root_with(
        &self,
        root: &RootCgroup,
        signal: &Arc<RootSignal>,
        child_id: Option<u32>,
        wait_for_stop: impl FnOnce(i32) -> Result<(), ResourceError>,
        write_membership: impl FnOnce(&Path, i32) -> Result<(), ResourceError>,
        resume: impl FnOnce(i32) -> Result<(), ResourceError>,
    ) -> Result<i32, ResourceError> {
        let pid = i32::try_from(child_id.ok_or(ResourceError::MissingProcessId)?)
            .map_err(|_| ResourceError::InvalidCgroupData("child pid overflow".into()))?;
        wait_for_stop(pid)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Err(ResourceError::RunClosed);
        }
        Self::refresh_events(&mut state)?;
        write_membership(&root.path.join("cgroup.procs"), pid)?;
        let entry = state.roots.get_mut(&root.id).ok_or_else(|| {
            ResourceError::InvalidCgroupData("prepared root cgroup is missing".into())
        })?;
        entry.active = true;
        debug_assert!(Arc::ptr_eq(&entry.signal, signal));
        resume(pid)?;
        Ok(pid)
    }

    pub(super) fn terminate_root_with(
        &self,
        root: &RootCgroup,
        live_root_pid: Option<i32>,
        cleanup: impl FnOnce(&Path, Option<i32>) -> Result<(), ResourceError>,
    ) -> Result<(), ResourceError> {
        let _cleanup_guard = root
            .cleanup_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let accounting = (!state.cleaned)
            .then(|| Self::refresh_events(&mut state).err())
            .flatten();
        let Some(entry) = state.roots.get_mut(&root.id) else {
            return accounting.map_or(Ok(()), Err);
        };
        let was_active = entry.active;
        entry.active = false;
        drop(state);
        let mut cleaned = false;
        let result = run_cleanup_after_accounting(accounting, || {
            let result = cleanup(&root.path, live_root_pid);
            cleaned = result.is_ok();
            result
        });
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cleaned {
            state.roots.remove(&root.id);
        } else if let Some(entry) = state.roots.get_mut(&root.id) {
            entry.active = was_active;
        }
        result
    }

    pub(super) fn refresh_events(state: &mut RunState) -> Result<(), ResourceError> {
        for entry in state.roots.values_mut().filter(|entry| entry.active) {
            let next = read_events(&entry.root.path)?;
            let violations = violations_since(entry.counters, next);
            entry.counters = next;
            if violations != 0 {
                entry
                    .signal
                    .violations
                    .fetch_or(violations, Ordering::AcqRel);
            }
        }
        Ok(())
    }

    pub(super) fn close_with(
        &self,
        mut cleanup: impl FnMut(&Path, Option<i32>) -> Result<(), ResourceError>,
    ) -> Result<(), ResourceError> {
        let _close_guard = self
            .close_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        if state.cleaned {
            return Ok(());
        }
        let mut roots: Vec<_> = state
            .roots
            .values()
            .map(|entry| Arc::clone(&entry.root))
            .collect();
        drop(state);
        roots.sort_by_key(|root| root.id);
        let _root_guards: Vec<_> = roots
            .iter()
            .map(|root| {
                root.cleanup_gate
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
            })
            .collect();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let accounting = Self::refresh_events(&mut state).err();
        let active_roots: Vec<_> = state
            .roots
            .iter_mut()
            .filter_map(|(id, entry)| {
                let was_active = entry.active;
                entry.active = false;
                was_active.then_some(*id)
            })
            .collect();
        drop(state);
        let mut first_error = None;
        for root in &roots {
            match cleanup(&root.path, None) {
                Ok(()) => {
                    self.state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .roots
                        .remove(&root.id);
                }
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        let run_cleanup = cleanup(&self.path, None);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if run_cleanup.is_ok() {
            state.roots.clear();
            state.cleaned = true;
        } else {
            for id in active_roots {
                if let Some(entry) = state.roots.get_mut(&id) {
                    entry.active = true;
                }
            }
        }
        let cleanup = first_error.map_or(run_cleanup, Err);
        run_cleanup_after_accounting(accounting, || cleanup)
    }
}

fn read_events(path: &Path) -> Result<CgroupEventCounters, ResourceError> {
    let memory = fs::read(path.join("memory.events"))
        .map_err(|error| ResourceError::io("read cgroup memory events", error))?;
    let pids = fs::read(path.join("pids.events"))
        .map_err(|error| ResourceError::io("read cgroup process events", error))?;
    parse_cgroup_event_counters(&memory, &pids)
}
