use std::ffi::OsString;
#[cfg(any(target_os = "linux", test))]
use std::path::PathBuf;

#[cfg(any(target_os = "linux", test))]
use hoimin_core::ProcessTermination;
use hoimin_core::RunLimits;

use super::{PortableBackend, ResourceBackend, ResourceError};

#[cfg(any(target_os = "linux", test))]
const INTERNAL_LAUNCHER_ARG: &str = "--hoimin-internal-cgroup-launch";
#[cfg(any(target_os = "linux", test))]
const MEMORY_VIOLATION: u8 = 1;
#[cfg(any(target_os = "linux", test))]
const PROCESS_VIOLATION: u8 = 2;
#[cfg(any(target_os = "linux", test))]
const BOTH_VIOLATIONS: u8 = 3;

#[cfg(any(target_os = "linux", test))]
fn normalized_memory_limit(
    limit: u64,
    page_size: u64,
    page_counter_max_pages: u64,
) -> Result<u64, ResourceError> {
    if page_size == 0 {
        return Err(ResourceError::InvalidCgroupData(
            "system page size must be positive".into(),
        ));
    }
    let max_finite_pages = page_counter_max_pages
        .checked_sub(1)
        .filter(|pages| *pages > 0)
        .ok_or_else(|| {
            ResourceError::InvalidCgroupData("cgroup page counter has no finite range".into())
        })?;
    let max_finite_bytes = max_finite_pages.checked_mul(page_size).ok_or_else(|| {
        ResourceError::InvalidCgroupData("finite cgroup memory limit overflowed u64".into())
    })?;
    if limit < page_size {
        return Err(ResourceError::InvalidCgroupMemoryLimit {
            requested: limit,
            page_size,
        });
    }
    Ok((limit - limit % page_size).min(max_finite_bytes))
}

#[cfg(target_os = "linux")]
pub(crate) use platform::LinuxSupervisor;
#[cfg(target_os = "linux")]
pub use platform::{LinuxBackend, PendingCgroupCleanup};

#[derive(Clone, Debug)]
pub enum CgroupCapabilities {
    #[cfg(target_os = "linux")]
    Available(LinuxBackend),
    #[cfg(target_os = "linux")]
    CleanupPending(PendingCgroupCleanup),
    Unavailable(String),
}

#[must_use]
pub fn probe_linux_cgroup(limits: &RunLimits) -> CgroupCapabilities {
    #[cfg(target_os = "linux")]
    {
        platform::LinuxBackend::probe(limits)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = limits;
        CgroupCapabilities::Unavailable("cgroup v2 hard limits require Linux".into())
    }
}

#[cfg(target_os = "linux")]
#[must_use]
pub fn probe_linux_cgroup_with_launcher(
    limits: &RunLimits,
    launcher: OsString,
) -> CgroupCapabilities {
    platform::LinuxBackend::probe_with_launcher(limits, launcher)
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the Linux launcher consumes its iterator while non-Linux targets discard it"
)]
pub fn run_linux_launcher_from<I, T>(args: I) -> Option<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    #[cfg(target_os = "linux")]
    {
        platform::run_launcher(args)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        None
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CgroupEventCounters {
    pub memory_max: u64,
    pub oom: u64,
    pub oom_kill: u64,
    pub oom_group_kill: u64,
    pub pids_max: u64,
}

/// # Errors
///
/// Returns an error when hard cgroup limits are unavailable and best-effort memory is disallowed,
/// or when pending cgroup cleanup cannot complete.
pub fn select_linux_backend(
    capabilities: CgroupCapabilities,
    allow_best_effort_memory: bool,
) -> Result<ResourceBackend, ResourceError> {
    match capabilities {
        #[cfg(target_os = "linux")]
        CgroupCapabilities::Available(backend) => Ok(ResourceBackend::LinuxHard(backend)),
        #[cfg(target_os = "linux")]
        CgroupCapabilities::CleanupPending(pending) => {
            let reason = pending.reason().to_owned();
            match pending.retry_cleanup() {
                Ok(()) if allow_best_effort_memory => Ok(ResourceBackend::Portable(
                    PortableBackend::with_diagnostic(reason),
                )),
                Ok(()) => Err(ResourceError::BestEffortNotAllowed(reason)),
                Err(cleanup) => Err(ResourceError::CgroupCleanupPending {
                    reason,
                    cleanup: Box::new(cleanup),
                    pending,
                }),
            }
        }
        CgroupCapabilities::Unavailable(reason) => {
            if !allow_best_effort_memory {
                return Err(ResourceError::BestEffortNotAllowed(reason));
            }
            Ok(ResourceBackend::Portable(PortableBackend::with_diagnostic(
                reason,
            )))
        }
    }
}

/// # Errors
///
/// Returns an error when either cgroup event file is malformed or contains a non-numeric value.
pub fn parse_cgroup_event_counters(
    memory_events: &[u8],
    pids_events: &[u8],
) -> Result<CgroupEventCounters, ResourceError> {
    let memory = parse_named_counters(memory_events)?;
    let pids = parse_named_counters(pids_events)?;
    Ok(CgroupEventCounters {
        memory_max: value(&memory, b"max"),
        oom: value(&memory, b"oom"),
        oom_kill: value(&memory, b"oom_kill"),
        oom_group_kill: value(&memory, b"oom_group_kill"),
        pids_max: value(&pids, b"max"),
    })
}

fn parse_named_counters(input: &[u8]) -> Result<Vec<(&[u8], u64)>, ResourceError> {
    input
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut fields = line.split(u8::is_ascii_whitespace);
            let name = fields.next().unwrap_or_default();
            let raw_value = fields.next().ok_or_else(|| {
                ResourceError::InvalidCgroupData(String::from_utf8_lossy(line).into_owned())
            })?;
            if name.is_empty() || fields.any(|field| !field.is_empty()) {
                return Err(ResourceError::InvalidCgroupData(
                    String::from_utf8_lossy(line).into_owned(),
                ));
            }
            let raw_value = std::str::from_utf8(raw_value).map_err(|_| {
                ResourceError::InvalidCgroupData(String::from_utf8_lossy(line).into_owned())
            })?;
            let value = raw_value.parse::<u64>().map_err(|_| {
                ResourceError::InvalidCgroupData(String::from_utf8_lossy(line).into_owned())
            })?;
            Ok((name, value))
        })
        .collect()
}

fn value(counters: &[(&[u8], u64)], name: &[u8]) -> u64 {
    counters
        .iter()
        .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
        .unwrap_or_default()
}

#[cfg(any(target_os = "linux", test))]
fn violations_since(before: CgroupEventCounters, after: CgroupEventCounters) -> u8 {
    let memory = after.oom_kill > before.oom_kill;
    let processes = after.pids_max > before.pids_max;
    match (memory, processes) {
        (false, false) => 0,
        (true, false) => MEMORY_VIOLATION,
        (false, true) => PROCESS_VIOLATION,
        (true, true) => BOTH_VIOLATIONS,
    }
}

#[cfg(any(target_os = "linux", test))]
fn classify_violations(termination: ProcessTermination, violations: u8) -> ProcessTermination {
    let memory = violations & MEMORY_VIOLATION != 0;
    let processes = violations & PROCESS_VIOLATION != 0;
    match (memory, processes) {
        (false, false) => termination,
        (true, false | true) => ProcessTermination::OutOfMemory,
        (false, true) => ProcessTermination::ProcessLimit,
    }
}

#[cfg(any(target_os = "linux", test))]
fn wrap_launcher_argv(launcher: OsString, argv: Vec<OsString>) -> Vec<OsString> {
    let mut wrapped = Vec::with_capacity(argv.len() + 2);
    wrapped.push(launcher);
    wrapped.push(OsString::from(INTERNAL_LAUNCHER_ARG));
    wrapped.extend(argv);
    wrapped
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct UnifiedMount {
    root: PathBuf,
    mountpoint: PathBuf,
}

#[cfg(test)]
fn find_unified_mount(mountinfo: &[u8]) -> Result<UnifiedMount, ResourceError> {
    find_unified_mounts(mountinfo)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            ResourceError::InvalidCgroupData("no cgroup2 mount in /proc/self/mountinfo".into())
        })
}

#[cfg(any(target_os = "linux", test))]
fn find_unified_mounts(mountinfo: &[u8]) -> Result<Vec<UnifiedMount>, ResourceError> {
    let mut mounts = Vec::new();
    for line in mountinfo.split(|byte| *byte == b'\n') {
        let Some(separator) = line.windows(3).position(|window| window == b" - ") else {
            continue;
        };
        let before = &line[..separator];
        let after = &line[separator + 3..];
        if after.split(|byte| *byte == b' ').next() != Some(&b"cgroup2"[..]) {
            continue;
        }
        let mut fields = before.split(|byte| *byte == b' ');
        let root = fields
            .nth(3)
            .ok_or_else(|| ResourceError::InvalidCgroupData("mountinfo root".into()))?;
        let mountpoint = fields
            .next()
            .ok_or_else(|| ResourceError::InvalidCgroupData("mountinfo mount point".into()))?;
        mounts.push(UnifiedMount {
            root: path_from_bytes(unescape_mount_field(root)?)?,
            mountpoint: path_from_bytes(unescape_mount_field(mountpoint)?)?,
        });
    }
    Ok(mounts)
}

#[cfg(any(target_os = "linux", test))]
fn resolve_unified_cgroup(
    mount: &UnifiedMount,
    current: &std::path::Path,
) -> Result<PathBuf, ResourceError> {
    use std::path::Component;

    for path in [&mount.root, &mount.mountpoint, current] {
        if !path.has_root()
            || path.components().any(|component| {
                !matches!(
                    component,
                    Component::Prefix(_) | Component::RootDir | Component::Normal(_)
                )
            })
        {
            return Err(ResourceError::InvalidCgroupData(format!(
                "unsafe cgroup path {}",
                path.display()
            )));
        }
    }
    let relative = current.strip_prefix(&mount.root).map_err(|_| {
        ResourceError::InvalidCgroupData(format!(
            "current cgroup {} is outside mounted root {}",
            current.display(),
            mount.root.display()
        ))
    })?;
    Ok(mount.mountpoint.join(relative))
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CleanupStrategy {
    KernelRecursiveKill,
    VerifiedLiveGroupThenMembershipPidLoop,
    VerifiedMembershipPidLoop,
}

#[cfg(any(target_os = "linux", test))]
fn cleanup_strategy(cgroup_kill_succeeded: bool, live_root_owned: bool) -> CleanupStrategy {
    if cgroup_kill_succeeded {
        CleanupStrategy::KernelRecursiveKill
    } else if live_root_owned {
        CleanupStrategy::VerifiedLiveGroupThenMembershipPidLoop
    } else {
        CleanupStrategy::VerifiedMembershipPidLoop
    }
}

#[cfg(any(target_os = "linux", test))]
fn begin_cgroup_member_cleanup<KernelGroupKill, ProcessGroupKill, NumericPidKill>(
    live_root_pid: Option<i32>,
    kernel_group_kill: KernelGroupKill,
    process_group_kill: ProcessGroupKill,
    numeric_pid_kill: NumericPidKill,
) -> Result<CleanupStrategy, ResourceError>
where
    KernelGroupKill: FnOnce() -> bool,
    ProcessGroupKill: FnOnce(i32) -> Result<(), ResourceError>,
    NumericPidKill: FnOnce() -> Result<(), ResourceError>,
{
    let strategy = cleanup_strategy(kernel_group_kill(), live_root_pid.is_some());
    if strategy == CleanupStrategy::VerifiedLiveGroupThenMembershipPidLoop {
        process_group_kill(live_root_pid.expect("strategy requires a live pid"))?;
    }
    if strategy != CleanupStrategy::KernelRecursiveKill {
        numeric_pid_kill()?;
    }
    Ok(strategy)
}

#[cfg(any(target_os = "linux", test))]
fn finish_cgroup_member_cleanup<NumericPidKill, CgroupTreeEmpty, WaitForCleanup>(
    strategy: CleanupStrategy,
    mut numeric_pid_kill: NumericPidKill,
    mut cgroup_tree_empty: CgroupTreeEmpty,
    mut wait_for_cleanup: WaitForCleanup,
) -> Result<(), ResourceError>
where
    NumericPidKill: FnMut() -> Result<(), ResourceError>,
    CgroupTreeEmpty: FnMut() -> Result<bool, ResourceError>,
    WaitForCleanup: FnMut() -> Result<(), ResourceError>,
{
    loop {
        if cgroup_tree_empty()? {
            return Ok(());
        }
        if strategy != CleanupStrategy::KernelRecursiveKill {
            numeric_pid_kill()?;
        }
        wait_for_cleanup()?;
    }
}

#[cfg(any(target_os = "linux", test))]
fn run_cleanup_after_accounting<F>(
    accounting: Option<ResourceError>,
    cleanup: F,
) -> Result<(), ResourceError>
where
    F: FnOnce() -> Result<(), ResourceError>,
{
    let cleanup = cleanup();
    match (accounting, cleanup) {
        (None, cleanup) => cleanup,
        (Some(accounting), Ok(())) => Err(accounting),
        (Some(accounting), Err(cleanup)) => Err(ResourceError::CgroupAccountingAndCleanup {
            accounting: Box::new(accounting),
            cleanup: Box::new(cleanup),
        }),
    }
}

#[cfg(any(target_os = "linux", test))]
fn unescape_mount_field(input: &[u8]) -> Result<Vec<u8>, ResourceError> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] != b'\\' {
            output.push(input[index]);
            index += 1;
            continue;
        }
        if index + 3 >= input.len()
            || !input[index + 1..=index + 3]
                .iter()
                .all(|byte| matches!(byte, b'0'..=b'7'))
        {
            return Err(ResourceError::InvalidCgroupData(
                "invalid mountinfo escape".into(),
            ));
        }
        let value = (input[index + 1] - b'0') * 64
            + (input[index + 2] - b'0') * 8
            + (input[index + 3] - b'0');
        output.push(value);
        index += 4;
    }
    Ok(output)
}

#[cfg(any(target_os = "linux", test))]
fn remove_cgroup_dir(path: &std::path::Path) -> Result<(), ResourceError> {
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ResourceError::CgroupCleanup {
            path: path.to_owned(),
            source,
        }),
    }
}

#[cfg(all(any(target_os = "linux", test), unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "The parser shares a fallible interface with platform-specific path decoding."
)]
fn path_from_bytes(bytes: Vec<u8>) -> Result<PathBuf, ResourceError> {
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(target_os = "linux")]
mod platform {
    use std::collections::HashMap;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use hoimin_core::{ProcessLimits, ProcessTermination, ResourceMode, RunLimits};
    use tokio::process::{Child, Command};
    use uuid::Uuid;

    use super::{
        CgroupCapabilities, CgroupEventCounters, INTERNAL_LAUNCHER_ARG,
        begin_cgroup_member_cleanup, classify_violations, find_unified_mounts,
        finish_cgroup_member_cleanup, normalized_memory_limit, parse_cgroup_event_counters,
        path_from_bytes, remove_cgroup_dir, resolve_unified_cgroup, run_cleanup_after_accounting,
        violations_since, wrap_launcher_argv,
    };
    use crate::resource::{ProcessSupervisor, ResourceError};

    const CONTROL_WAIT: Duration = Duration::from_secs(1);
    const CGROUP2_SUPER_MAGIC: libc::c_long = 0x6367_7270;

    #[derive(Clone, Debug)]
    pub struct PendingCgroupCleanup {
        inner: Arc<PendingCleanupInner>,
        reason: Arc<str>,
    }

    #[allow(
        clippy::missing_errors_doc,
        clippy::must_use_candidate,
        reason = "These methods are public only within the private Linux cgroup backend module."
    )]
    impl PendingCgroupCleanup {
        fn new(paths: Vec<PathBuf>, reason: String, child: Option<std::process::Child>) -> Self {
            Self {
                inner: Arc::new(PendingCleanupInner {
                    paths,
                    child: Mutex::new(child),
                    cleaned: std::sync::atomic::AtomicBool::new(false),
                }),
                reason: reason.into(),
            }
        }

        pub fn reason(&self) -> &str {
            &self.reason
        }

        pub fn retry_cleanup(&self) -> Result<(), ResourceError> {
            if self.inner.cleaned.load(Ordering::Acquire) {
                return Ok(());
            }
            for path in &self.inner.paths {
                let _ = cleanup_cgroup(path, None);
            }
            {
                let mut child = self
                    .inner
                    .child
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(owned) = child.as_mut() {
                    match owned.try_wait() {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            let _ = owned.kill();
                            bounded_reap(owned)?;
                        }
                        Err(error) => {
                            return Err(ResourceError::io(
                                "poll pending cgroup probe child",
                                error,
                            ));
                        }
                    }
                    child.take();
                }
            }
            let mut cleanup_error = None;
            for path in &self.inner.paths {
                if let Err(error) = cleanup_cgroup(path, None) {
                    cleanup_error.get_or_insert(error);
                }
            }
            if let Some(error) = cleanup_error {
                return Err(error);
            }
            self.inner.cleaned.store(true, Ordering::Release);
            Ok(())
        }
    }

    #[derive(Debug)]
    struct PendingCleanupInner {
        paths: Vec<PathBuf>,
        child: Mutex<Option<std::process::Child>>,
        cleaned: std::sync::atomic::AtomicBool,
    }

    impl Drop for PendingCleanupInner {
        fn drop(&mut self) {
            if self.cleaned.load(Ordering::Acquire) {
                return;
            }
            for path in &self.paths {
                let _ = cleanup_cgroup(path, None);
            }
            let child = self
                .child
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let child_clean = match child.as_mut() {
                Some(owned) => {
                    let _ = owned.kill();
                    if bounded_reap(owned).is_ok() {
                        child.take();
                        true
                    } else {
                        false
                    }
                }
                None => true,
            };
            let mut paths_clean = true;
            for path in &self.paths {
                if cleanup_cgroup(path, None).is_err() {
                    paths_clean = false;
                }
            }
            if child_clean && paths_clean {
                self.cleaned.store(true, Ordering::Release);
            }
        }
    }

    #[derive(Clone, Debug)]
    pub struct LinuxBackend {
        run: Arc<LinuxRunCgroup>,
        launcher: OsString,
    }

    #[allow(
        clippy::missing_errors_doc,
        clippy::must_use_candidate,
        reason = "These methods are public only within the private Linux cgroup backend module."
    )]
    impl LinuxBackend {
        pub fn probe(limits: &RunLimits) -> CgroupCapabilities {
            match std::env::current_exe() {
                Ok(launcher) => Self::probe_with_launcher(limits, launcher.into_os_string()),
                Err(error) => CgroupCapabilities::Unavailable(format!(
                    "locate hoimin launcher failed: {error}"
                )),
            }
        }

        pub fn probe_with_launcher(limits: &RunLimits, launcher: OsString) -> CgroupCapabilities {
            let mut diagnostics = Vec::new();
            let result = Self::probe_with_diagnostics(limits, launcher, &mut diagnostics);
            match result {
                Ok(backend) => CgroupCapabilities::Available(backend),
                Err(ResourceError::CgroupCleanupPending { pending, .. }) => {
                    CgroupCapabilities::CleanupPending(pending)
                }
                Err(error) => {
                    diagnostics.push(error.to_string());
                    CgroupCapabilities::Unavailable(diagnostics.join("; "))
                }
            }
        }

        fn probe_with_diagnostics(
            _limits: &RunLimits,
            launcher: OsString,
            diagnostics: &mut Vec<String>,
        ) -> Result<Self, ResourceError> {
            let mountinfo = fs::read("/proc/self/mountinfo")
                .map_err(|error| ResourceError::io("read cgroup mountinfo", error))?;
            let current = current_unified_path(
                &fs::read("/proc/self/cgroup")
                    .map_err(|error| ResourceError::io("read current unified cgroup", error))?,
            )?;
            let parent = resolve_current_membership(&mountinfo, &current)?;
            enable_required_controllers(&parent, diagnostics)?;

            let run_path = create_unique_child(&parent, "hoimin")?;
            let setup = (|| {
                enable_required_controllers(&run_path, diagnostics)?;
                verify_migration(&run_path, &launcher)?;
                Ok(LinuxRunCgroup {
                    path: run_path.clone(),
                    diagnostics: diagnostics.clone(),
                    state: Mutex::new(RunState::default()),
                    close_gate: Mutex::new(()),
                })
            })();
            let run = match setup {
                Ok(run) => run,
                Err(error) => {
                    if matches!(&error, ResourceError::CgroupCleanupPending { .. }) {
                        return Err(error);
                    }
                    if let Err(cleanup) = cleanup_cgroup(&run_path, None) {
                        let reason = format!(
                            "cgroup setup failed: {error}; recursive cleanup for {} is pending",
                            run_path.display()
                        );
                        let pending =
                            PendingCgroupCleanup::new(vec![run_path], reason.clone(), None);
                        return Err(ResourceError::CgroupCleanupPending {
                            reason,
                            cleanup: Box::new(cleanup),
                            pending,
                        });
                    }
                    return Err(error);
                }
            };
            Ok(Self {
                run: Arc::new(run),
                launcher,
            })
        }

        pub fn mode(&self) -> ResourceMode {
            ResourceMode::Hard
        }

        pub fn diagnostics(&self) -> &[String] {
            &self.run.diagnostics
        }

        #[doc(hidden)]
        pub fn run_cgroup_path_for_tests(&self) -> PathBuf {
            self.run.path.clone()
        }

        pub(crate) fn wrap_argv(&self, argv: Vec<OsString>) -> Vec<OsString> {
            wrap_launcher_argv(self.launcher.clone(), argv)
        }

        pub(crate) fn prepare(
            &self,
            _command: &mut Command,
            limits: ProcessLimits,
        ) -> Result<ProcessSupervisor, ResourceError> {
            self.run.prepare_root(limits)
        }

        pub fn close(&self) -> Result<(), ResourceError> {
            self.run.close()
        }
    }

    #[derive(Debug)]
    struct LinuxRunCgroup {
        path: PathBuf,
        diagnostics: Vec<String>,
        state: Mutex<RunState>,
        close_gate: Mutex<()>,
    }

    #[derive(Debug, Default)]
    struct RunState {
        closed: bool,
        cleaned: bool,
        roots: HashMap<Uuid, RootEntry>,
    }

    #[derive(Debug)]
    struct RootEntry {
        root: Arc<RootCgroup>,
        signal: Arc<RootSignal>,
        active: bool,
        counters: CgroupEventCounters,
    }

    #[derive(Debug)]
    struct RootCgroup {
        id: Uuid,
        path: PathBuf,
        cleanup_gate: Mutex<()>,
    }

    #[derive(Debug, Default)]
    struct RootSignal {
        violations: AtomicU8,
    }

    impl LinuxRunCgroup {
        fn prepare_root(
            self: &Arc<Self>,
            limits: ProcessLimits,
        ) -> Result<ProcessSupervisor, ResourceError> {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.closed {
                return Err(ResourceError::RunClosed);
            }
            // Reject invalid limits before creating a root cgroup or launching a child.
            system_memory_limit(limits.max_memory_bytes)?;
            Self::refresh_events(&mut state)?;
            let path = create_unique_child(&self.path, "root")?;
            let mut diagnostics = Vec::new();
            write_memory_limit(
                &path.join("memory.max"),
                limits.max_memory_bytes,
                &mut diagnostics,
            )?;
            write_exact_limit(
                &path.join("pids.max"),
                limits.max_processes.to_string().as_bytes(),
            )?;
            write_exact_limit(&path.join("memory.oom.group"), b"1")?;
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
            Ok(ProcessSupervisor::Linux(LinuxSupervisor {
                run: Arc::clone(self),
                root,
                signal,
                pid: None,
                terminated: false,
            }))
        }

        fn attach_root(
            &self,
            root: &RootCgroup,
            signal: &Arc<RootSignal>,
            child: &Child,
        ) -> Result<i32, ResourceError> {
            let pid = i32::try_from(child.id().ok_or(ResourceError::MissingProcessId)?)
                .map_err(|_| ResourceError::InvalidCgroupData("child pid overflow".into()))?;
            wait_for_launcher_stop(pid)?;
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.closed {
                return Err(ResourceError::RunClosed);
            }
            Self::refresh_events(&mut state)?;
            fs::write(root.path.join("cgroup.procs"), pid.to_string())
                .map_err(|error| ResourceError::io("attach stopped root to cgroup", error))?;
            let entry = state.roots.get_mut(&root.id).ok_or_else(|| {
                ResourceError::InvalidCgroupData("prepared root cgroup is missing".into())
            })?;
            entry.active = true;
            debug_assert!(Arc::ptr_eq(&entry.signal, signal));
            // SAFETY: pid is the stopped launcher child owned by this supervisor.
            if unsafe { libc::kill(pid, libc::SIGCONT) } != 0 {
                return Err(ResourceError::io(
                    "continue attached cgroup launcher",
                    io::Error::last_os_error(),
                ));
            }
            Ok(pid)
        }

        fn classify_root(
            &self,
            signal: &RootSignal,
            termination: ProcessTermination,
        ) -> Result<ProcessTermination, ResourceError> {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !state.cleaned {
                Self::refresh_events(&mut state)?;
            }
            let violations = signal.violations.load(Ordering::Acquire);
            Ok(classify_violations(termination, violations))
        }

        fn terminate_root(
            &self,
            root: &RootCgroup,
            live_root_pid: Option<i32>,
        ) -> Result<(), ResourceError> {
            self.terminate_root_with(root, live_root_pid, cleanup_cgroup)
        }

        fn terminate_root_with(
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

        fn refresh_events(state: &mut RunState) -> Result<(), ResourceError> {
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

        fn close(&self) -> Result<(), ResourceError> {
            self.close_with(cleanup_cgroup)
        }

        fn close_with(
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

    impl Drop for LinuxRunCgroup {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }

    #[derive(Debug)]
    pub(crate) struct LinuxSupervisor {
        run: Arc<LinuxRunCgroup>,
        root: Arc<RootCgroup>,
        signal: Arc<RootSignal>,
        pid: Option<i32>,
        terminated: bool,
    }

    impl LinuxSupervisor {
        pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
            self.pid = Some(self.run.attach_root(&self.root, &self.signal, child)?);
            Ok(())
        }

        pub(crate) fn classify(
            &mut self,
            termination: ProcessTermination,
        ) -> Result<ProcessTermination, ResourceError> {
            self.run.classify_root(&self.signal, termination)
        }

        pub(crate) fn terminate(&mut self, live_root_owned: bool) -> Result<(), ResourceError> {
            if self.terminated {
                return Ok(());
            }
            let live_root_pid = live_root_owned.then_some(self.pid).flatten();
            self.run.terminate_root(&self.root, live_root_pid)?;
            self.terminated = true;
            Ok(())
        }
    }

    impl Drop for LinuxSupervisor {
        fn drop(&mut self) {
            let _ = self.run.terminate_root(&self.root, None);
        }
    }

    pub(super) fn run_launcher<I, T>(args: I) -> Option<i32>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        let mut args = args.into_iter().map(Into::into);
        let _program = args.next()?;
        if args.next().as_deref() != Some(OsStr::new(INTERNAL_LAUNCHER_ARG)) {
            return None;
        }
        let Some(target) = args.next() else {
            eprintln!("internal cgroup launcher is missing its target");
            return Some(126);
        };
        // SAFETY: these calls affect only this pre-runtime launcher process.
        if unsafe { libc::setpgid(0, 0) } != 0 {
            eprintln!(
                "internal cgroup launcher setpgid failed: {}",
                io::Error::last_os_error()
            );
            return Some(126);
        }
        // SAFETY: SIGSTOP is raised in the launcher before any target code executes.
        if unsafe { libc::raise(libc::SIGSTOP) } != 0 {
            eprintln!(
                "internal cgroup launcher stop failed: {}",
                io::Error::last_os_error()
            );
            return Some(126);
        }
        let error = std::process::Command::new(target).args(args).exec();
        eprintln!("internal cgroup launcher exec failed: {error}");
        Some(126)
    }

    fn current_unified_path(cgroup: &[u8]) -> Result<PathBuf, ResourceError> {
        let line = cgroup
            .split(|byte| *byte == b'\n')
            .find(|line| line.starts_with(b"0::"))
            .ok_or_else(|| {
                ResourceError::InvalidCgroupData("no unified entry in /proc/self/cgroup".into())
            })?;
        path_from_bytes(line[3..].to_vec())
    }

    fn resolve_current_membership(
        mountinfo: &[u8],
        current: &Path,
    ) -> Result<PathBuf, ResourceError> {
        let mounts = find_unified_mounts(mountinfo)?;
        let mut failures = Vec::new();
        for mount in mounts {
            let candidate = (|| {
                let resolved = resolve_unified_cgroup(&mount, current)?;
                let mountpoint = fs::canonicalize(&mount.mountpoint)
                    .map_err(|error| ResourceError::io("canonicalize cgroup2 mount", error))?;
                let parent = fs::canonicalize(resolved)
                    .map_err(|error| ResourceError::io("canonicalize delegated cgroup", error))?;
                if !parent.starts_with(&mountpoint) {
                    return Err(ResourceError::InvalidCgroupData(
                        "current cgroup escapes the cgroup2 mount".into(),
                    ));
                }
                verify_cgroup2_filesystem(&parent)?;
                verify_member(&parent, std::process::id())?;
                Ok(parent)
            })();
            match candidate {
                Ok(parent) => return Ok(parent),
                Err(error) => failures.push(error.to_string()),
            }
        }
        Err(ResourceError::InvalidCgroupData(if failures.is_empty() {
            "no cgroup2 mount in /proc/self/mountinfo".into()
        } else {
            format!(
                "no cgroup2 mount matched this process membership: {}",
                failures.join("; ")
            )
        }))
    }

    fn verify_cgroup2_filesystem(path: &Path) -> Result<(), ResourceError> {
        let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            ResourceError::InvalidCgroupData("cgroup path contains a NUL byte".into())
        })?;
        // SAFETY: path is a live NUL-terminated pathname and stats points to writable storage.
        let mut stats: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: arguments satisfy statfs(2)'s pointer requirements.
        if unsafe { libc::statfs(path.as_ptr(), std::ptr::addr_of_mut!(stats)) } != 0 {
            return Err(ResourceError::io(
                "verify cgroup2 filesystem",
                io::Error::last_os_error(),
            ));
        }
        if stats.f_type != CGROUP2_SUPER_MAGIC {
            return Err(ResourceError::InvalidCgroupData(
                "resolved membership path is not on cgroup2fs".into(),
            ));
        }
        Ok(())
    }

    fn verify_member(path: &Path, pid: u32) -> Result<(), ResourceError> {
        let procs = fs::read(path.join("cgroup.procs"))
            .map_err(|error| ResourceError::io("read cgroup membership", error))?;
        if parse_member_pids(&procs)?.contains(&pid) {
            Ok(())
        } else {
            Err(ResourceError::InvalidCgroupData(format!(
                "pid {pid} is not a direct member of {}",
                path.display()
            )))
        }
    }

    fn enable_required_controllers(
        parent: &Path,
        diagnostics: &mut Vec<String>,
    ) -> Result<(), ResourceError> {
        let available = fs::read(parent.join("cgroup.controllers"))
            .map_err(|error| ResourceError::io("read delegated cgroup controllers", error))?;
        for required in [b"memory".as_slice(), b"pids".as_slice()] {
            if !available
                .split(u8::is_ascii_whitespace)
                .any(|controller| controller == required)
            {
                return Err(ResourceError::InvalidCgroupData(format!(
                    "delegated cgroup lacks {} controller",
                    String::from_utf8_lossy(required)
                )));
            }
        }
        let control_path = parent.join("cgroup.subtree_control");
        let enabled = fs::read(&control_path)
            .map_err(|error| ResourceError::io("read delegated subtree controllers", error))?;
        let missing: Vec<_> = [b"memory".as_slice(), b"pids".as_slice()]
            .into_iter()
            .filter(|required| {
                !enabled
                    .split(u8::is_ascii_whitespace)
                    .any(|controller| controller == *required)
            })
            .collect();
        if !missing.is_empty() {
            let command = missing
                .iter()
                .map(|controller| format!("+{}", String::from_utf8_lossy(controller)))
                .collect::<Vec<_>>()
                .join(" ");
            fs::write(&control_path, command.as_bytes()).map_err(|error| {
                diagnostics.push(format!(
                    "failed enabling delegated controllers with `{command}`; existing subtree_control is `{}`",
                    String::from_utf8_lossy(&enabled)
                ));
                ResourceError::io("enable delegated cgroup controllers", error)
            })?;
            diagnostics.push(format!(
                "enabled delegated subtree controllers with `{command}`; the parent setting is intentionally retained"
            ));
        }
        let verified = fs::read(&control_path)
            .map_err(|error| ResourceError::io("verify delegated subtree controllers", error))?;
        for required in [b"memory".as_slice(), b"pids".as_slice()] {
            if !verified
                .split(u8::is_ascii_whitespace)
                .any(|controller| controller == required)
            {
                return Err(ResourceError::InvalidCgroupData(format!(
                    "delegated subtree did not enable {}",
                    String::from_utf8_lossy(required)
                )));
            }
        }
        Ok(())
    }

    fn create_unique_child(parent: &Path, prefix: &str) -> Result<PathBuf, ResourceError> {
        for _ in 0..8 {
            let path = parent.join(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                Uuid::new_v4()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(ResourceError::io("create delegated cgroup", error)),
            }
        }
        Err(ResourceError::InvalidCgroupData(
            "could not allocate a unique delegated cgroup".into(),
        ))
    }

    fn write_exact_limit(path: &Path, value: &[u8]) -> Result<(), ResourceError> {
        fs::write(path, value)
            .map_err(|error| ResourceError::io("write cgroup hard limit", error))?;
        let actual =
            fs::read(path).map_err(|error| ResourceError::io("verify cgroup hard limit", error))?;
        if actual.trim_ascii() != value {
            return Err(ResourceError::InvalidCgroupData(format!(
                "cgroup limit readback mismatch for {}: requested {}, read {}",
                path.display(),
                String::from_utf8_lossy(value),
                String::from_utf8_lossy(actual.trim_ascii())
            )));
        }
        Ok(())
    }

    fn system_page_size() -> Result<u64, ResourceError> {
        // SAFETY: `_SC_PAGESIZE` is a side-effect-free process configuration query.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        u64::try_from(page_size).map_err(|_| {
            ResourceError::InvalidCgroupData(format!(
                "system page size query returned invalid value {page_size}"
            ))
        })
    }

    fn system_memory_limit(requested: u64) -> Result<(u64, u64), ResourceError> {
        let page_size = system_page_size()?;
        if page_size == 0 {
            return Err(ResourceError::InvalidCgroupData(
                "system page size must be positive".into(),
            ));
        }
        let long_max = u64::try_from(libc::c_long::MAX)
            .expect("Linux signed long maximum is positive and fits u64");
        let page_counter_max_pages = if cfg!(target_pointer_width = "32") {
            long_max
        } else {
            long_max / page_size
        };
        let effective = normalized_memory_limit(requested, page_size, page_counter_max_pages)?;
        Ok((effective, page_size))
    }

    fn write_memory_limit(
        path: &Path,
        requested: u64,
        diagnostics: &mut Vec<String>,
    ) -> Result<(), ResourceError> {
        let (effective, page_size) = system_memory_limit(requested)?;
        let value = effective.to_string();
        write_exact_limit(path, value.as_bytes())?;
        if effective != requested {
            diagnostics.push(format!(
                "cgroup memory.max normalized down from {requested} to {effective} bytes for {page_size}-byte pages"
            ));
        }
        Ok(())
    }

    fn read_events(path: &Path) -> Result<CgroupEventCounters, ResourceError> {
        let memory = fs::read(path.join("memory.events"))
            .map_err(|error| ResourceError::io("read cgroup memory events", error))?;
        let pids = fs::read(path.join("pids.events"))
            .map_err(|error| ResourceError::io("read cgroup process events", error))?;
        parse_cgroup_event_counters(&memory, &pids)
    }

    fn verify_migration(run_path: &Path, launcher: &OsStr) -> Result<(), ResourceError> {
        let probe_path = create_unique_child(run_path, "probe")?;
        let child = match std::process::Command::new(launcher)
            .arg(INTERNAL_LAUNCHER_ARG)
            .arg(launcher)
            .arg("--help")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = remove_cgroup_dir(&probe_path);
                return Err(ResourceError::io(
                    "spawn stopped cgroup migration probe",
                    error,
                ));
            }
        };
        let guard = MigrationProbeGuard {
            child: Some(child),
            probe_path,
            run_path: run_path.to_path_buf(),
            finished: false,
        };
        let pid = guard
            .child
            .as_ref()
            .expect("migration probe child is owned until cleanup")
            .id();
        let operation = (|| {
            let pid_i32 = i32::try_from(pid).map_err(|_| {
                ResourceError::InvalidCgroupData("migration probe pid overflow".into())
            })?;
            wait_for_launcher_stop(pid_i32)?;
            fs::write(guard.probe_path.join("cgroup.procs"), pid.to_string())
                .map_err(|error| ResourceError::io("migrate stopped cgroup probe", error))?;
            verify_member(&guard.probe_path, pid)?;
            Ok(())
        })();
        guard.finish(operation)
    }

    struct MigrationProbeGuard {
        child: Option<std::process::Child>,
        probe_path: PathBuf,
        run_path: PathBuf,
        finished: bool,
    }

    impl MigrationProbeGuard {
        fn finish(mut self, operation: Result<(), ResourceError>) -> Result<(), ResourceError> {
            let mut cleanup_error = cleanup_cgroup(&self.probe_path, None).err();
            if let Some(child) = self.child.as_mut() {
                match child.try_wait() {
                    Ok(None) => {
                        if let Err(error) = child.kill() {
                            cleanup_error.get_or_insert_with(|| {
                                ResourceError::io("kill cgroup migration probe", error)
                            });
                        }
                    }
                    Ok(Some(_)) => {}
                    Err(error) => {
                        cleanup_error.get_or_insert_with(|| {
                            ResourceError::io("poll cgroup migration probe", error)
                        });
                    }
                }
                if let Err(error) = bounded_reap(child) {
                    cleanup_error.get_or_insert(error);
                }
            }
            if let Err(error) = cleanup_cgroup(&self.probe_path, None) {
                cleanup_error.get_or_insert(error);
            }
            let Some(cleanup) = cleanup_error else {
                self.finished = true;
                return operation;
            };
            let cleanup = match operation {
                Ok(()) => cleanup,
                Err(operation) => ResourceError::CgroupOperationAndCleanup {
                    operation: Box::new(operation),
                    cleanup: Box::new(cleanup),
                },
            };
            let reason = format!(
                "cgroup migration probe cleanup for {} is pending",
                self.probe_path.display()
            );
            let pending = PendingCgroupCleanup::new(
                vec![self.probe_path.clone(), self.run_path.clone()],
                reason.clone(),
                self.child.take(),
            );
            self.finished = true;
            Err(ResourceError::CgroupCleanupPending {
                reason,
                cleanup: Box::new(cleanup),
                pending,
            })
        }
    }

    impl Drop for MigrationProbeGuard {
        fn drop(&mut self) {
            if self.finished {
                return;
            }
            if let Some(child) = self.child.as_mut() {
                let _ = child.kill();
                let _ = bounded_reap(child);
            }
            let _ = cleanup_cgroup(&self.probe_path, None);
            let _ = cleanup_cgroup(&self.run_path, None);
        }
    }

    fn bounded_reap(child: &mut std::process::Child) -> Result<(), ResourceError> {
        let deadline = Instant::now() + CONTROL_WAIT;
        loop {
            match child
                .try_wait()
                .map_err(|error| ResourceError::io("reap cgroup migration probe", error))?
            {
                Some(_) => return Ok(()),
                None if Instant::now() >= deadline => {
                    return Err(ResourceError::io(
                        "reap cgroup migration probe",
                        io::Error::new(io::ErrorKind::TimedOut, "migration probe reap timed out"),
                    ));
                }
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    fn wait_for_launcher_stop(pid: i32) -> Result<(), ResourceError> {
        let launcher_id = libc::id_t::try_from(pid)
            .map_err(|_| ResourceError::InvalidCgroupData("invalid cgroup launcher pid".into()))?;
        let deadline = Instant::now() + CONTROL_WAIT;
        loop {
            let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
            // SAFETY: launcher_id identifies our child, info points to writable storage, and
            // WNOWAIT leaves child status owned by the caller's Child handle.
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    launcher_id,
                    info.as_mut_ptr(),
                    libc::WSTOPPED | libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result < 0 {
                return Err(ResourceError::io(
                    "wait for stopped cgroup launcher",
                    io::Error::last_os_error(),
                ));
            }
            // SAFETY: waitid returned success and therefore initialized the siginfo storage.
            let info = unsafe { info.assume_init() };
            // SAFETY: waitid populated siginfo with a SIGCHLD payload, or left the zeroed
            // si_pid sentinel unchanged because WNOHANG found no matching state.
            let observed_pid = unsafe { info.si_pid() };
            if observed_pid != 0 {
                // SAFETY: a nonzero si_pid denotes a populated SIGCHLD payload.
                let status = unsafe { info.si_status() };
                if info.si_code == libc::CLD_STOPPED && status == libc::SIGSTOP {
                    return Ok(());
                }
                return Err(ResourceError::InvalidCgroupData(
                    "cgroup launcher did not stop before target execution".into(),
                ));
            }
            if Instant::now() >= deadline {
                return Err(ResourceError::io(
                    "wait for stopped cgroup launcher",
                    io::Error::new(io::ErrorKind::TimedOut, "launcher stop timed out"),
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn cleanup_cgroup(path: &Path, live_root_pid: Option<i32>) -> Result<(), ResourceError> {
        match path.try_exists() {
            Ok(false) => return Ok(()),
            Ok(true) => {}
            Err(error) => return Err(cleanup_error(path, error)),
        }
        let strategy = begin_cgroup_member_cleanup(
            live_root_pid,
            || fs::write(path.join("cgroup.kill"), b"1").is_ok(),
            |pid| kill_verified_live_group(path, pid),
            || kill_all_listed_pids(path),
        )?;
        let deadline = Instant::now() + CONTROL_WAIT;
        finish_cgroup_member_cleanup(
            strategy,
            || kill_all_listed_pids(path),
            || cgroup_tree_empty(path),
            || {
                if Instant::now() >= deadline {
                    return Err(cleanup_error(
                        path,
                        io::Error::new(io::ErrorKind::TimedOut, "cgroup remained populated"),
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
                Ok(())
            },
        )?;
        let mut tree = cgroup_tree(path)?;
        tree.sort_by_key(|candidate| std::cmp::Reverse(candidate.components().count()));
        for cgroup in tree {
            remove_cgroup_dir(&cgroup)?;
        }
        Ok(())
    }

    fn kill_all_listed_pids(path: &Path) -> Result<(), ResourceError> {
        kill_all_listed_pids_with(path, open_pidfd, send_sigkill)
    }

    fn kill_all_listed_pids_with<Handle, OpenPidfd, SendSignal>(
        path: &Path,
        mut open_pidfd: OpenPidfd,
        mut send_signal: SendSignal,
    ) -> Result<(), ResourceError>
    where
        OpenPidfd: FnMut(i32) -> io::Result<Handle>,
        SendSignal: FnMut(&Handle) -> io::Result<()>,
    {
        for cgroup in cgroup_tree(path)? {
            let procs = fs::read(cgroup.join("cgroup.procs"))
                .map_err(|error| cleanup_error(&cgroup, error))?;
            for pid in parse_member_pids(&procs)? {
                signal_verified_member(
                    &cgroup,
                    pid,
                    &mut open_pidfd,
                    |path| {
                        let procs = fs::read(path.join("cgroup.procs"))
                            .map_err(|error| cleanup_error(path, error))?;
                        parse_member_pids(&procs)
                    },
                    &mut send_signal,
                )?;
            }
        }
        Ok(())
    }

    fn signal_verified_member<Handle, OpenPidfd, ReadMembership, SendSignal>(
        cgroup: &Path,
        pid: u32,
        open_pidfd: OpenPidfd,
        read_membership: ReadMembership,
        send_signal: SendSignal,
    ) -> Result<(), ResourceError>
    where
        OpenPidfd: FnOnce(i32) -> io::Result<Handle>,
        ReadMembership: FnOnce(&Path) -> Result<Vec<u32>, ResourceError>,
        SendSignal: FnOnce(&Handle) -> io::Result<()>,
    {
        let numeric_pid = i32::try_from(pid)
            .map_err(|_| cleanup_error(cgroup, io::Error::other("cgroup pid overflow")))?;
        let handle = match open_pidfd(numeric_pid) {
            Ok(handle) => handle,
            Err(error) if error.raw_os_error() == Some(libc::ESRCH) => return Ok(()),
            Err(error) => return Err(cleanup_error(cgroup, error)),
        };
        if !read_membership(cgroup)?.contains(&pid) {
            return Ok(());
        }
        match send_signal(&handle) {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
            Err(error) => Err(cleanup_error(cgroup, error)),
        }
    }

    fn open_pidfd(pid: i32) -> io::Result<std::os::fd::OwnedFd> {
        use std::os::fd::FromRawFd;

        // SAFETY: pidfd_open receives a valid scalar PID and zero flags.
        let result = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0_u32) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        let descriptor = i32::try_from(result).expect("Linux file descriptors fit c_int");
        // SAFETY: pidfd_open returned a new owned descriptor with close-on-exec set.
        Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(descriptor) })
    }

    fn send_sigkill(pidfd: &std::os::fd::OwnedFd) -> io::Result<()> {
        use std::os::fd::AsRawFd;

        // SAFETY: pidfd remains live for the call, the signal is valid, and null info with zero
        // flags requests kill(2)-equivalent signal metadata.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                pidfd.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0_u32,
            )
        };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn kill_verified_live_group(path: &Path, pid: i32) -> Result<(), ResourceError> {
        let procs =
            fs::read(path.join("cgroup.procs")).map_err(|error| cleanup_error(path, error))?;
        let pid_u32 = u32::try_from(pid)
            .map_err(|_| cleanup_error(path, io::Error::other("invalid live root pid")))?;
        if !parse_member_pids(&procs)?.contains(&pid_u32) {
            return Ok(());
        }
        // SAFETY: pid is still owned by the unwaited Child and was just verified in this cgroup.
        let group = unsafe { libc::getpgid(pid) };
        if group < 0 {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(libc::ESRCH) {
                Ok(())
            } else {
                Err(cleanup_error(path, error))
            };
        }
        if group != pid {
            return Ok(());
        }
        // SAFETY: the live owned root is its verified process-group leader.
        if unsafe { libc::kill(-pid, libc::SIGKILL) } != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(cleanup_error(path, error));
            }
        }
        Ok(())
    }

    fn parse_member_pids(procs: &[u8]) -> Result<Vec<u32>, ResourceError> {
        procs
            .split(u8::is_ascii_whitespace)
            .filter(|raw| !raw.is_empty())
            .map(|raw| {
                std::str::from_utf8(raw)
                    .ok()
                    .and_then(|raw| raw.parse::<u32>().ok())
                    .ok_or_else(|| {
                        ResourceError::InvalidCgroupData("invalid cgroup.procs pid".into())
                    })
            })
            .collect()
    }

    fn cgroup_tree_empty(path: &Path) -> Result<bool, ResourceError> {
        for cgroup in cgroup_tree(path)? {
            let procs = fs::read(cgroup.join("cgroup.procs"))
                .map_err(|error| cleanup_error(&cgroup, error))?;
            if !procs.trim_ascii().is_empty() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn cgroup_tree(root: &Path) -> Result<Vec<PathBuf>, ResourceError> {
        match root.try_exists() {
            Ok(false) => return Ok(Vec::new()),
            Ok(true) => {}
            Err(error) => return Err(cleanup_error(root, error)),
        }
        let mut tree = vec![root.to_owned()];
        let mut index = 0;
        while index < tree.len() {
            let parent = tree[index].clone();
            let entries = match fs::read_dir(&parent) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    index += 1;
                    continue;
                }
                Err(error) => return Err(cleanup_error(&parent, error)),
            };
            for entry in entries {
                let entry = entry.map_err(|error| cleanup_error(&parent, error))?;
                let file_type = entry
                    .file_type()
                    .map_err(|error| cleanup_error(&entry.path(), error))?;
                if file_type.is_symlink() {
                    return Err(cleanup_error(
                        &entry.path(),
                        io::Error::other("symlink inside owned cgroup subtree"),
                    ));
                }
                if file_type.is_dir() {
                    let child = entry.path();
                    if !child.starts_with(root) {
                        return Err(cleanup_error(
                            &child,
                            io::Error::other("cgroup traversal escaped owned subtree"),
                        ));
                    }
                    tree.push(child);
                }
            }
            index += 1;
        }
        Ok(tree)
    }

    fn cleanup_error(path: &Path, source: io::Error) -> ResourceError {
        ResourceError::CgroupCleanup {
            path: path.to_owned(),
            source,
        }
    }

    #[cfg(test)]
    mod tests {
        use std::cell::{Cell, RefCell};
        use std::io;
        use std::os::unix::process::ExitStatusExt;
        use std::path::Path;
        use std::process::{Child, Command};
        use std::sync::{Arc, Mutex, mpsc};
        use std::time::Duration;

        use super::{
            CgroupEventCounters, LinuxRunCgroup, RootCgroup, RootEntry, RootSignal, RunState,
            kill_all_listed_pids_with, open_pidfd, send_sigkill, signal_verified_member,
            system_page_size, wait_for_launcher_stop, write_memory_limit,
        };

        fn fake_run(path: &Path) -> Arc<LinuxRunCgroup> {
            std::fs::create_dir(path).unwrap();
            Arc::new(LinuxRunCgroup {
                path: path.to_owned(),
                diagnostics: Vec::new(),
                state: Mutex::new(RunState::default()),
                close_gate: Mutex::new(()),
            })
        }

        fn fake_root(run: &LinuxRunCgroup) -> Arc<RootCgroup> {
            let id = uuid::Uuid::new_v4();
            let path = run.path.join(id.to_string());
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join("memory.events"), b"oom_kill 0\n").unwrap();
            std::fs::write(path.join("pids.events"), b"max 0\n").unwrap();
            let root = Arc::new(RootCgroup {
                id,
                path,
                cleanup_gate: Mutex::new(()),
            });
            run.state.lock().unwrap().roots.insert(
                id,
                RootEntry {
                    root: Arc::clone(&root),
                    signal: Arc::new(RootSignal::default()),
                    active: true,
                    counters: CgroupEventCounters::default(),
                },
            );
            root
        }

        fn remove_fake_cgroup(path: &Path, _pid: Option<i32>) -> Result<(), super::ResourceError> {
            match std::fs::remove_dir_all(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(super::cleanup_error(path, error)),
            }
        }

        #[test]
        fn root_cleanup_releases_state_and_allows_sibling_termination() {
            let directory = tempfile::tempdir().unwrap();
            let run = fake_run(&directory.path().join("run"));
            let root = fake_root(&run);
            let sibling = fake_root(&run);
            let (paused_tx, paused_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            std::thread::scope(|scope| {
                let run = &run;
                let root = &root;
                let sibling = &sibling;
                let terminating = scope.spawn(move || {
                    run.terminate_root_with(root, Some(41), |path, pid| {
                        assert_eq!(pid, Some(41));
                        remove_fake_cgroup(path, pid)?;
                        paused_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(())
                    })
                });
                paused_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let state_available = run.state.try_lock().is_ok();
                let (sibling_tx, sibling_rx) = mpsc::channel();
                let sibling_termination = scope.spawn(move || {
                    sibling_tx
                        .send(run.terminate_root_with(sibling, None, remove_fake_cgroup))
                        .unwrap();
                });
                let sibling_result = sibling_rx.recv_timeout(Duration::from_secs(2));
                release_tx.send(()).unwrap();
                terminating.join().unwrap().unwrap();
                sibling_termination.join().unwrap();
                assert!(
                    state_available,
                    "cleanup must release the shared state mutex"
                );
                sibling_result
                    .expect("sibling cleanup must progress while the first cleanup is paused")
                    .expect("refresh must skip the root whose counters have been removed");
            });
            assert!(run.state.lock().unwrap().roots.is_empty());
        }

        #[test]
        fn close_cleanup_releases_state_and_publishes_closed_before_waiting() {
            let directory = tempfile::tempdir().unwrap();
            let run = fake_run(&directory.path().join("run"));
            let root = fake_root(&run);
            let observed = Cell::new(false);
            run.close_with(|path, pid| {
                if path == root.path
                    && let Ok(state) = run.state.try_lock()
                {
                    assert!(state.closed);
                    assert!(!state.roots[&root.id].active);
                    observed.set(true);
                }
                remove_fake_cgroup(path, pid)
            })
            .unwrap();
            assert!(
                observed.get(),
                "close cleanup must release the shared state mutex"
            );
            assert!(run.state.lock().unwrap().cleaned);
        }

        #[test]
        fn close_waits_for_inflight_root_cleanup_without_holding_state() {
            let directory = tempfile::tempdir().unwrap();
            let run = fake_run(&directory.path().join("run"));
            let root = fake_root(&run);
            let (paused_tx, paused_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            std::thread::scope(|scope| {
                let run = &run;
                let root = &root;
                let terminating = scope.spawn(move || {
                    run.terminate_root_with(root, None, |path, pid| {
                        remove_fake_cgroup(path, pid)?;
                        paused_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(())
                    })
                });
                paused_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let closing = scope.spawn(|| run.close_with(remove_fake_cgroup));
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let closed = loop {
                    if run.state.try_lock().is_ok_and(|state| state.closed) {
                        break true;
                    }
                    if std::time::Instant::now() >= deadline {
                        break false;
                    }
                    std::thread::yield_now();
                };
                let blocked_on_root = root.cleanup_gate.try_lock().is_err();
                release_tx.send(()).unwrap();
                terminating.join().unwrap().unwrap();
                closing.join().unwrap().unwrap();
                assert!(
                    closed,
                    "close must publish closed and release state before waiting"
                );
                assert!(blocked_on_root);
            });
            assert!(run.state.lock().unwrap().cleaned);
        }

        #[test]
        fn close_keeps_all_root_gates_through_recursive_run_cleanup() {
            let directory = tempfile::tempdir().unwrap();
            let run = fake_run(&directory.path().join("run"));
            let first = fake_root(&run);
            let second = fake_root(&run);
            let mut calls = Vec::new();
            let error = run
                .close_with(|path, pid| {
                    calls.push(path.to_owned());
                    assert!(run.close_gate.try_lock().is_err());
                    assert!(first.cleanup_gate.try_lock().is_err());
                    assert!(second.cleanup_gate.try_lock().is_err());
                    assert!(run.state.try_lock().is_ok());
                    if path == first.path {
                        Err(super::cleanup_error(path, io::Error::other("root retry")))
                    } else {
                        remove_fake_cgroup(path, pid)
                    }
                })
                .unwrap_err();
            assert!(matches!(error, super::ResourceError::CgroupCleanup { .. }));
            let mut expected_roots = [first.as_ref(), second.as_ref()];
            expected_roots.sort_by_key(|root| root.id);
            assert_eq!(
                calls,
                [
                    expected_roots[0].path.clone(),
                    expected_roots[1].path.clone(),
                    run.path.clone()
                ]
            );
            assert!(run.state.lock().unwrap().cleaned);
            run.close_with(|_, _| panic!("successful recursive cleanup must not repeat"))
                .unwrap();
        }

        #[test]
        fn failed_root_cleanup_restores_accounting_and_remains_retryable() {
            let directory = tempfile::tempdir().unwrap();
            let run = fake_run(&directory.path().join("run"));
            let root = fake_root(&run);
            let signal = Arc::clone(&run.state.lock().unwrap().roots[&root.id].signal);
            run.terminate_root_with(&root, None, |path, _| {
                assert!(!run.state.lock().unwrap().roots[&root.id].active);
                Err(super::cleanup_error(path, io::Error::other("retry")))
            })
            .unwrap_err();
            assert!(run.state.lock().unwrap().roots[&root.id].active);
            std::fs::write(root.path.join("memory.events"), b"oom_kill 1\n").unwrap();
            assert_eq!(
                run.classify_root(&signal, hoimin_core::ProcessTermination::Exit(0))
                    .unwrap(),
                hoimin_core::ProcessTermination::OutOfMemory,
            );
            run.terminate_root_with(&root, None, remove_fake_cgroup)
                .unwrap();
            assert!(run.state.lock().unwrap().roots.is_empty());
        }

        #[test]
        fn failed_close_restores_only_failed_roots_and_retries_run_removal() {
            let directory = tempfile::tempdir().unwrap();
            let run = fake_run(&directory.path().join("run"));
            let root = fake_root(&run);
            let sibling = fake_root(&run);
            run.close_with(|path, pid| {
                if path == sibling.path {
                    remove_fake_cgroup(path, pid)
                } else {
                    Err(super::cleanup_error(path, io::Error::other("retry")))
                }
            })
            .unwrap_err();
            {
                let state = run.state.lock().unwrap();
                assert!(state.closed);
                assert!(!state.cleaned);
                assert_eq!(state.roots.len(), 1);
                assert!(state.roots[&root.id].active);
            }
            let limits = hoimin_core::ProcessLimits {
                timeout: Duration::from_secs(1),
                max_output_bytes: 1024,
                max_memory_bytes: 4096,
                max_processes: 1,
            };
            assert!(matches!(
                run.prepare_root(limits),
                Err(super::ResourceError::RunClosed)
            ));
            run.close_with(remove_fake_cgroup).unwrap();
            assert!(run.state.lock().unwrap().cleaned);
            assert!(!run.path.exists());
        }

        #[test]
        fn accounting_error_still_commits_successful_root_cleanup() {
            let directory = tempfile::tempdir().unwrap();
            let run = fake_run(&directory.path().join("run"));
            let root = fake_root(&run);
            std::fs::write(root.path.join("memory.events"), b"oom_kill invalid\n").unwrap();
            let error = run
                .terminate_root_with(&root, None, remove_fake_cgroup)
                .unwrap_err();
            assert!(matches!(error, super::ResourceError::InvalidCgroupData(_)));
            assert!(run.state.lock().unwrap().roots.is_empty());
            run.terminate_root_with(&root, None, |_, _| {
                panic!("removed root must not be cleaned twice")
            })
            .unwrap();
        }

        struct DropRecorder<'a>(&'a Cell<usize>);

        impl Drop for DropRecorder<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        #[test]
        fn cleanup_rechecks_fake_cgroup_membership_after_opening_pidfd() {
            let cgroup = tempfile::tempdir().unwrap();
            let membership = cgroup.path().join("cgroup.procs");
            std::fs::write(&membership, b"41\n").unwrap();
            let opened = Cell::new(0);

            kill_all_listed_pids_with(
                cgroup.path(),
                |pid| {
                    assert_eq!(pid, 41);
                    opened.set(opened.get() + 1);
                    std::fs::write(&membership, b"").unwrap();
                    Ok("original process")
                },
                |_| panic!("a process that migrated away must not be signaled"),
            )
            .unwrap();

            assert_eq!(opened.get(), 1);
        }

        #[test]
        fn pidfd_signal_targets_the_acquired_process() {
            let mut child = ReapingChild(
                Command::new("sleep")
                    .arg("60")
                    .spawn()
                    .expect("spawn pidfd target"),
            );
            let pid = i32::try_from(child.0.id()).expect("fixture pid fits i32");

            let pidfd = open_pidfd(pid).expect("acquire child pidfd");
            send_sigkill(&pidfd).expect("signal child through pidfd");
            let status = child.0.wait().expect("reap pidfd target");

            assert_eq!(status.signal(), Some(libc::SIGKILL));
        }

        #[test]
        fn member_is_revalidated_after_pidfd_acquisition() {
            let events = RefCell::new(Vec::new());
            let signaled = Cell::new(false);

            signal_verified_member(
                Path::new("/owned"),
                41,
                |pid| {
                    events.borrow_mut().push("open");
                    assert_eq!(pid, 41);
                    Ok("original process")
                },
                |_| {
                    events.borrow_mut().push("membership");
                    Ok(Vec::new())
                },
                |_| {
                    events.borrow_mut().push("signal");
                    signaled.set(true);
                    Ok(())
                },
            )
            .unwrap();

            assert_eq!(*events.borrow(), ["open", "membership"]);
            assert!(!signaled.get());
        }

        #[test]
        fn signal_uses_the_handle_acquired_before_membership_validation() {
            signal_verified_member(
                Path::new("/owned"),
                41,
                |_| Ok("original process"),
                |_| Ok(vec![41]),
                |handle| {
                    assert_eq!(*handle, "original process");
                    Ok(())
                },
            )
            .unwrap();
        }

        #[test]
        fn process_exit_before_pidfd_acquisition_is_not_a_cleanup_error() {
            let membership_read = Cell::new(false);
            let signaled = Cell::new(false);

            signal_verified_member(
                Path::new("/owned"),
                41,
                |_| Err::<(), _>(std::io::Error::from_raw_os_error(libc::ESRCH)),
                |_| {
                    membership_read.set(true);
                    Ok(vec![41])
                },
                |()| {
                    signaled.set(true);
                    Ok(())
                },
            )
            .unwrap();

            assert!(!membership_read.get());
            assert!(!signaled.get());
        }

        #[test]
        fn process_exit_after_pidfd_acquisition_is_not_a_cleanup_error() {
            signal_verified_member(
                Path::new("/owned"),
                41,
                |_| Ok("original process"),
                |_| Ok(vec![41]),
                |_| Err(std::io::Error::from_raw_os_error(libc::ESRCH)),
            )
            .unwrap();
        }

        #[test]
        fn pidfd_open_failures_are_fail_closed() {
            for error_code in [libc::ENOSYS, libc::EPERM, libc::EMFILE, libc::EINVAL] {
                let membership_read = Cell::new(false);
                let signaled = Cell::new(false);
                let error = signal_verified_member(
                    Path::new("/owned"),
                    41,
                    |_| Err::<(), _>(io::Error::from_raw_os_error(error_code)),
                    |_| {
                        membership_read.set(true);
                        Ok(vec![41])
                    },
                    |()| {
                        signaled.set(true);
                        Ok(())
                    },
                )
                .unwrap_err();

                let super::ResourceError::CgroupCleanup { source, .. } = error else {
                    panic!("unexpected error: {error}");
                };
                assert_eq!(source.raw_os_error(), Some(error_code));
                assert!(!membership_read.get());
                assert!(!signaled.get());
            }
        }

        #[test]
        fn pidfd_signal_failures_are_fail_closed_and_release_the_handle() {
            for error_code in [libc::ENOSYS, libc::EPERM, libc::EINVAL] {
                let drops = Cell::new(0);
                let error = signal_verified_member(
                    Path::new("/owned"),
                    41,
                    |_| Ok(DropRecorder(&drops)),
                    |_| Ok(vec![41]),
                    |_| Err(io::Error::from_raw_os_error(error_code)),
                )
                .unwrap_err();

                let super::ResourceError::CgroupCleanup { source, .. } = error else {
                    panic!("unexpected error: {error}");
                };
                assert_eq!(source.raw_os_error(), Some(error_code));
                assert_eq!(drops.get(), 1);
            }
        }

        #[test]
        fn page_sized_memory_limits_are_written_and_verified() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("memory.max");
            let page_size = system_page_size().unwrap();
            for requested in [page_size, page_size + 1] {
                let mut diagnostics = Vec::new();
                write_memory_limit(&path, requested, &mut diagnostics).unwrap();
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    page_size.to_string()
                );
                assert_eq!(diagnostics.is_empty(), requested == page_size);
            }
        }

        #[test]
        fn subpage_memory_rejection_does_not_write_the_limit_file() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("memory.max");
            std::fs::write(&path, b"unchanged").unwrap();
            let mut diagnostics = Vec::new();
            let error =
                write_memory_limit(&path, system_page_size().unwrap() - 1, &mut diagnostics)
                    .unwrap_err();
            assert!(error.to_string().contains("--max-memory"));
            assert_eq!(std::fs::read(&path).unwrap(), b"unchanged");
            assert!(diagnostics.is_empty());
        }

        #[test]
        fn subpage_memory_rejection_precedes_root_cgroup_creation() {
            let directory = tempfile::tempdir().unwrap();
            let run = std::sync::Arc::new(LinuxRunCgroup {
                path: directory.path().to_owned(),
                diagnostics: Vec::new(),
                state: std::sync::Mutex::new(RunState::default()),
                close_gate: Mutex::new(()),
            });
            let error = run
                .prepare_root(hoimin_core::ProcessLimits {
                    timeout: std::time::Duration::from_secs(1),
                    max_output_bytes: 1024,
                    max_memory_bytes: 100,
                    max_processes: 1,
                })
                .unwrap_err();
            assert!(error.to_string().contains("--max-memory"), "{error}");
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
            assert!(run.state.lock().unwrap().roots.is_empty());
        }

        struct ReapingChild(Child);

        impl Drop for ReapingChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        #[test]
        fn premature_launcher_exit_remains_reapable_by_its_child_owner() {
            let mut child = ReapingChild(
                Command::new("sh")
                    .args(["-c", "exit 23"])
                    .spawn()
                    .expect("spawn premature launcher exit fixture"),
            );
            let pid = i32::try_from(child.0.id()).expect("fixture pid fits i32");

            let error = wait_for_launcher_stop(pid).unwrap_err();
            let status = child
                .0
                .wait()
                .expect("launcher status remains owned by Child");

            assert_eq!(status.code(), Some(23));
            assert!(
                error
                    .to_string()
                    .contains("cgroup launcher did not stop before target execution")
            );
        }

        #[test]
        fn stopped_launcher_is_observed_and_reaped_only_by_its_child_owner() {
            let mut child = ReapingChild(
                Command::new("sh")
                    .args(["-c", "kill -STOP $$; exit 0"])
                    .spawn()
                    .expect("spawn stopped launcher fixture"),
            );
            let pid = i32::try_from(child.0.id()).expect("fixture pid fits i32");

            wait_for_launcher_stop(pid).expect("observe SIGSTOP");
            // SAFETY: pid still identifies the stopped child owned by this test.
            assert_eq!(unsafe { libc::kill(pid, libc::SIGCONT) }, 0);
            let status = child.0.wait().expect("reap continued launcher");

            assert_eq!(status.code(), Some(0));
        }
    }
}

#[cfg(all(any(target_os = "linux", test), not(unix)))]
fn path_from_bytes(bytes: Vec<u8>) -> Result<PathBuf, ResourceError> {
    String::from_utf8(bytes)
        .map(PathBuf::from)
        .map_err(|_| ResourceError::InvalidCgroupData("non-UTF-8 cgroup mount path".into()))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::ffi::OsString;

    use hoimin_core::ProcessTermination;

    use super::{
        BOTH_VIOLATIONS, CgroupEventCounters, CleanupStrategy, MEMORY_VIOLATION, PROCESS_VIOLATION,
        ResourceError, begin_cgroup_member_cleanup, classify_violations, cleanup_strategy,
        find_unified_mount, find_unified_mounts, finish_cgroup_member_cleanup,
        normalized_memory_limit, remove_cgroup_dir, resolve_unified_cgroup,
        run_cleanup_after_accounting, violations_since, wrap_launcher_argv,
    };

    #[derive(Default)]
    struct CleanupSignalRecorder {
        numeric_pid_calls: Cell<usize>,
        process_group_calls: Cell<usize>,
    }

    impl CleanupSignalRecorder {
        fn signal_numeric_pids(&self) {
            self.numeric_pid_calls.set(self.numeric_pid_calls.get() + 1);
        }

        fn signal_process_group(&self, _pid: i32) {
            self.process_group_calls
                .set(self.process_group_calls.get() + 1);
        }

        fn numeric_pid_calls(&self) -> usize {
            self.numeric_pid_calls.get()
        }

        fn process_group_calls(&self) -> usize {
            self.process_group_calls.get()
        }
    }

    #[derive(Default)]
    struct KernelGroupKillRecorder {
        kill_calls: Cell<usize>,
    }

    impl KernelGroupKillRecorder {
        fn kill(&self) -> bool {
            self.kill_calls.set(self.kill_calls.get() + 1);
            true
        }

        fn kill_calls(&self) -> usize {
            self.kill_calls.get()
        }
    }

    #[test]
    fn memory_limit_normalization_rejects_subpage_values() {
        for page_size in [4_096, 16_384, 65_536] {
            for requested in [0, 1, 100, page_size - 1] {
                let error = normalized_memory_limit(requested, page_size, 1_000_000)
                    .expect_err("subpage memory must not normalize to zero");
                let message = error.to_string();
                assert!(message.contains("--max-memory"), "{message}");
                assert!(message.contains(&requested.to_string()), "{message}");
                assert!(message.contains(&page_size.to_string()), "{message}");
            }
        }
    }

    #[test]
    fn memory_limit_normalization_accepts_one_page_boundary() {
        for page_size in [4_096, 16_384, 65_536] {
            for requested in [page_size, page_size + 1, 2 * page_size - 1] {
                assert_eq!(
                    normalized_memory_limit(requested, page_size, 2).unwrap(),
                    page_size
                );
            }
        }
    }

    #[test]
    fn memory_limit_normalization_rounds_down_to_the_host_page_size() {
        let page_counter_max_pages = 1_000_000_000;
        for (requested, page_size, expected) in [
            (1_000_000_000, 4_096, 999_997_440),
            (1024 * 1024 * 1024, 4_096, 1024 * 1024 * 1024),
            (1_000_000_000, 65_536, 999_948_288),
        ] {
            assert_eq!(
                normalized_memory_limit(requested, page_size, page_counter_max_pages).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn memory_limit_normalization_stays_below_the_kernel_max_sentinel() {
        assert_eq!(normalized_memory_limit(u64::MAX, 4_096, 4).unwrap(), 12_288);
    }

    #[test]
    fn memory_limit_normalization_rejects_invalid_kernel_boundaries() {
        let zero_page = normalized_memory_limit(1_000_000_000, 0, 4).unwrap_err();
        let no_finite_range = normalized_memory_limit(1_000_000_000, 4_096, 0).unwrap_err();
        let only_zero = normalized_memory_limit(1_000_000_000, 4_096, 1).unwrap_err();

        assert!(matches!(zero_page, ResourceError::InvalidCgroupData(_)));
        assert!(matches!(only_zero, ResourceError::InvalidCgroupData(_)));
        assert!(matches!(
            no_finite_range,
            ResourceError::InvalidCgroupData(_)
        ));
    }

    #[test]
    fn launcher_wrapper_preserves_native_target_arguments() {
        let original = vec![
            OsString::from("target"),
            OsString::from("space & | $() ; literal"),
        ];

        let wrapped = wrap_launcher_argv(OsString::from("hoimin"), original.clone());

        assert_eq!(wrapped[0], OsString::from("hoimin"));
        assert_eq!(
            wrapped[1],
            OsString::from("--hoimin-internal-cgroup-launch")
        );
        assert_eq!(&wrapped[2..], original);
    }

    #[test]
    fn finds_and_unescapes_the_unified_cgroup_mount() {
        let mountinfo = b"31 23 0:27 /delegated /sys/fs/cgroup\\040delegated rw,nosuid,nodev,noexec,relatime - cgroup2 cgroup rw\n";
        let mount = find_unified_mount(mountinfo).unwrap();

        assert_eq!(mount.root, std::path::PathBuf::from("/delegated"));
        assert_eq!(
            mount.mountpoint,
            std::path::PathBuf::from("/sys/fs/cgroup delegated")
        );
        assert_eq!(
            resolve_unified_cgroup(&mount, std::path::Path::new("/delegated/worker")).unwrap(),
            std::path::PathBuf::from("/sys/fs/cgroup delegated/worker")
        );
    }

    #[test]
    fn retains_all_cgroup2_mount_candidates() {
        let mountinfo = b"31 23 0:27 /other /sys/fs/other rw - cgroup2 cgroup rw\n32 23 0:28 /delegated /sys/fs/cgroup rw - cgroup2 cgroup rw\n";

        let mounts = find_unified_mounts(mountinfo).unwrap();

        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].root, std::path::PathBuf::from("/other"));
        assert_eq!(mounts[1].root, std::path::PathBuf::from("/delegated"));
    }

    #[test]
    fn pressure_counters_do_not_create_terminal_memory_evidence() {
        let before = CgroupEventCounters {
            memory_max: 5,
            oom: 7,
            oom_kill: 11,
            oom_group_kill: 13,
            pids_max: 17,
        };
        let observations = [
            CgroupEventCounters {
                memory_max: 6,
                ..before
            },
            CgroupEventCounters { oom: 8, ..before },
            CgroupEventCounters {
                oom_group_kill: 14,
                ..before
            },
            CgroupEventCounters {
                memory_max: 6,
                oom: 8,
                oom_group_kill: 14,
                ..before
            },
        ];
        for after in observations {
            assert_eq!(violations_since(before, after), 0);
        }
    }

    #[test]
    fn terminal_event_deltas_use_strict_monotonic_increases() {
        let before = CgroupEventCounters {
            memory_max: 5,
            oom: 7,
            oom_kill: 11,
            oom_group_kill: 13,
            pids_max: 17,
        };

        assert_eq!(violations_since(before, before), 0);
        assert_eq!(
            violations_since(
                before,
                CgroupEventCounters {
                    oom_kill: 12,
                    ..before
                }
            ),
            MEMORY_VIOLATION,
        );
        assert_eq!(
            violations_since(
                before,
                CgroupEventCounters {
                    pids_max: 18,
                    ..before
                }
            ),
            PROCESS_VIOLATION,
        );
        assert_eq!(
            violations_since(
                before,
                CgroupEventCounters {
                    oom_kill: 12,
                    pids_max: 18,
                    ..before
                }
            ),
            BOTH_VIOLATIONS,
        );
        assert_eq!(
            violations_since(
                before,
                CgroupEventCounters {
                    oom_kill: 10,
                    pids_max: 18,
                    ..before
                }
            ),
            PROCESS_VIOLATION,
        );
        assert_eq!(
            violations_since(
                before,
                CgroupEventCounters {
                    oom_kill: 12,
                    pids_max: 16,
                    ..before
                }
            ),
            MEMORY_VIOLATION,
        );
    }

    #[test]
    fn known_violation_bits_map_with_memory_precedence() {
        let incoming = ProcessTermination::Exit(7);
        for (bits, expected) in [
            (0, incoming),
            (MEMORY_VIOLATION, ProcessTermination::OutOfMemory),
            (PROCESS_VIOLATION, ProcessTermination::ProcessLimit),
            (BOTH_VIOLATIONS, ProcessTermination::OutOfMemory),
            (4, incoming),
            (4 | MEMORY_VIOLATION, ProcessTermination::OutOfMemory),
            (4 | PROCESS_VIOLATION, ProcessTermination::ProcessLimit),
            (4 | BOTH_VIOLATIONS, ProcessTermination::OutOfMemory),
        ] {
            assert_eq!(classify_violations(incoming, bits), expected);
        }
    }

    #[test]
    fn cgroup_removal_failure_is_typed_and_retryable() {
        let parent = tempfile::tempdir().unwrap();
        let cgroup = parent.path().join("run-cgroup");
        std::fs::create_dir(&cgroup).unwrap();
        let obstruction = cgroup.join("still-populated");
        std::fs::write(&obstruction, b"pid").unwrap();

        let error = remove_cgroup_dir(&cgroup).unwrap_err();
        assert!(matches!(error, ResourceError::CgroupCleanup { .. }));
        assert!(cgroup.exists());

        std::fs::remove_file(obstruction).unwrap();
        remove_cgroup_dir(&cgroup).unwrap();
        assert!(!cgroup.exists());
    }

    #[test]
    fn successful_kernel_kill_never_signals_a_numeric_pid_or_group() {
        let signals = CleanupSignalRecorder::default();
        let kernel_group = KernelGroupKillRecorder::default();
        let emptiness_checks = Cell::new(0);

        let strategy = begin_cgroup_member_cleanup(
            None,
            || kernel_group.kill(),
            |pid| {
                signals.signal_process_group(pid);
                Ok(())
            },
            || {
                signals.signal_numeric_pids();
                Ok(())
            },
        )
        .unwrap();
        finish_cgroup_member_cleanup(
            strategy,
            || {
                signals.signal_numeric_pids();
                Ok(())
            },
            || {
                let check = emptiness_checks.get();
                emptiness_checks.set(check + 1);
                Ok(check > 0)
            },
            || Ok(()),
        )
        .unwrap();

        assert_eq!(signals.numeric_pid_calls(), 0);
        assert_eq!(signals.process_group_calls(), 0);
        assert_eq!(kernel_group.kill_calls(), 1);
    }

    #[test]
    fn failed_kernel_kill_uses_only_verified_fallbacks() {
        assert_eq!(
            cleanup_strategy(false, false),
            CleanupStrategy::VerifiedMembershipPidLoop
        );
        assert_eq!(
            cleanup_strategy(false, true),
            CleanupStrategy::VerifiedLiveGroupThenMembershipPidLoop
        );
    }

    #[test]
    fn accounting_failure_does_not_skip_cleanup() {
        let mut cleaned = false;
        let accounting = ResourceError::InvalidCgroupData("events unavailable".into());

        let error = run_cleanup_after_accounting(Some(accounting), || {
            cleaned = true;
            Ok(())
        })
        .unwrap_err();

        assert!(cleaned);
        assert!(matches!(error, ResourceError::InvalidCgroupData(_)));
    }

    #[test]
    fn accounting_and_cleanup_failures_are_combined() {
        let accounting = ResourceError::InvalidCgroupData("events unavailable".into());
        let cleanup = ResourceError::CgroupCleanup {
            path: std::path::PathBuf::from("run"),
            source: std::io::Error::other("still populated"),
        };

        let error = run_cleanup_after_accounting(Some(accounting), || Err(cleanup)).unwrap_err();

        assert!(matches!(
            error,
            ResourceError::CgroupAccountingAndCleanup { .. }
        ));
    }
}
