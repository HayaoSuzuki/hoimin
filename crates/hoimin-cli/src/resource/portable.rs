use std::io;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use hoimin_core::{ProcessLimits, ResourceMode};
use tokio::process::{Child, Command};

use super::{ProcessSupervisor, ResourceError};

#[cfg(all(windows, test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AttachFault {
    #[default]
    None,
    #[cfg(test)]
    AssignAfterDelay,
}

#[cfg(target_os = "macos")]
pub(super) const MACOS_BEST_EFFORT_DIAGNOSTIC: &str =
    "macOS uses process groups; max-memory is not enforced";

#[derive(Clone, Debug, Default)]
pub struct PortableBackend {
    diagnostic: Option<String>,
    classification_failures: Arc<AtomicU8>,
    termination_failures: Arc<AtomicU8>,
    #[cfg(all(windows, test))]
    attach_fault: AttachFault,
}

impl PortableBackend {
    /// # Errors
    ///
    /// Returns an error on Linux or macOS when best-effort memory limiting was not explicitly
    /// allowed.
    pub fn new(allow_best_effort_memory: bool) -> Result<Self, ResourceError> {
        #[cfg(target_os = "linux")]
        {
            if !allow_best_effort_memory {
                return Err(ResourceError::BestEffortNotAllowed(
                    "portable Linux uses per-process RLIMIT_AS and process groups".into(),
                ));
            }
            Ok(Self {
                diagnostic: None,
                classification_failures: Arc::new(AtomicU8::new(0)),
                termination_failures: Arc::new(AtomicU8::new(0)),
                #[cfg(all(windows, test))]
                attach_fault: AttachFault::None,
            })
        }
        #[cfg(target_os = "macos")]
        {
            if !allow_best_effort_memory {
                return Err(ResourceError::BestEffortNotAllowed(
                    MACOS_BEST_EFFORT_DIAGNOSTIC.into(),
                ));
            }
            Ok(Self::with_diagnostic(MACOS_BEST_EFFORT_DIAGNOSTIC.into()))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = allow_best_effort_memory;
            Ok(Self {
                diagnostic: None,
                classification_failures: Arc::new(AtomicU8::new(0)),
                termination_failures: Arc::new(AtomicU8::new(0)),
                #[cfg(all(windows, test))]
                attach_fault: AttachFault::None,
            })
        }
    }

    #[must_use]
    pub fn for_tests() -> Self {
        Self::default()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn for_tests_with_termination_failure() -> Self {
        Self {
            diagnostic: None,
            classification_failures: Arc::new(AtomicU8::new(0)),
            termination_failures: Arc::new(AtomicU8::new(1)),
            #[cfg(all(windows, test))]
            attach_fault: AttachFault::None,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn for_tests_with_classification_failure() -> Self {
        Self {
            diagnostic: None,
            classification_failures: Arc::new(AtomicU8::new(1)),
            termination_failures: Arc::new(AtomicU8::new(0)),
            #[cfg(all(windows, test))]
            attach_fault: AttachFault::None,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn for_tests_with_classification_and_termination_failure() -> Self {
        Self {
            diagnostic: None,
            classification_failures: Arc::new(AtomicU8::new(1)),
            termination_failures: Arc::new(AtomicU8::new(2)),
            #[cfg(all(windows, test))]
            attach_fault: AttachFault::None,
        }
    }

    pub(crate) fn with_diagnostic(diagnostic: String) -> Self {
        Self {
            diagnostic: Some(diagnostic),
            classification_failures: Arc::new(AtomicU8::new(0)),
            termination_failures: Arc::new(AtomicU8::new(0)),
            #[cfg(all(windows, test))]
            attach_fault: AttachFault::None,
        }
    }

    #[cfg(all(test, windows))]
    fn with_delayed_assignment_failure() -> Self {
        Self {
            diagnostic: None,
            classification_failures: Arc::new(AtomicU8::new(0)),
            termination_failures: Arc::new(AtomicU8::new(0)),
            attach_fault: AttachFault::AssignAfterDelay,
        }
    }

    #[must_use]
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    #[must_use]
    pub fn mode(&self) -> ResourceMode {
        ResourceMode::BestEffort
    }

    pub(crate) fn prepare(
        &self,
        command: &mut Command,
        limits: ProcessLimits,
    ) -> Result<ProcessSupervisor, ResourceError> {
        configure_command(command, limits)?;
        Ok(ProcessSupervisor::Portable(PortableSupervisor::new(
            Arc::clone(&self.classification_failures),
            Arc::clone(&self.termination_failures),
            #[cfg(all(windows, test))]
            self.attach_fault,
        )?))
    }
}

/// Authority over a Unix process group. After reap, a numeric PGID may be
/// recycled, so retained identifiers permit existence probes only. `None` does
/// not prove absence; that evidence belongs to `tree_quiescent`.
#[cfg(unix)]
#[derive(Debug)]
enum ProcessGroup {
    None,
    Signalable(i32),
    ProbeOnly(i32),
}

#[cfg(unix)]
impl ProcessGroup {
    fn record_root_reaped(&mut self) {
        if let Self::Signalable(group) = *self {
            *self = Self::ProbeOnly(group);
        }
    }

    fn probe_after_root_reap<F>(&mut self, probe: F) -> Result<bool, ResourceError>
    where
        F: FnOnce(i32) -> Result<bool, ResourceError>,
    {
        // Disarm before the fallible probe: retries and Drop must never signal
        // a numeric identifier after the root's ownership has ended.
        self.record_root_reaped();
        let Self::ProbeOnly(group) = *self else {
            return Ok(false);
        };
        let absent = probe(group)?;
        if absent {
            *self = Self::None;
        }
        Ok(absent)
    }
}

#[derive(Debug)]
pub(crate) struct PortableSupervisor {
    #[cfg(unix)]
    process_group: ProcessGroup,
    #[cfg(windows)]
    job: isize,
    #[cfg(all(windows, test))]
    attach_fault: AttachFault,
    terminated: bool,
    tree_quiescent: bool,
    classification_failures: Arc<AtomicU8>,
    termination_failures: Arc<AtomicU8>,
}

impl PortableSupervisor {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "Windows job-object creation is fallible while Unix construction is not."
    )]
    fn new(
        classification_failures: Arc<AtomicU8>,
        termination_failures: Arc<AtomicU8>,
        #[cfg(all(windows, test))] attach_fault: AttachFault,
    ) -> Result<Self, ResourceError> {
        #[cfg(windows)]
        let job = create_kill_on_close_job()?;
        Ok(Self {
            #[cfg(unix)]
            process_group: ProcessGroup::None,
            #[cfg(windows)]
            job,
            #[cfg(all(windows, test))]
            attach_fault,
            terminated: false,
            tree_quiescent: false,
            classification_failures,
            termination_failures,
        })
    }

    #[cfg_attr(
        not(unix),
        allow(
            clippy::unused_self,
            reason = "only Unix numeric process groups need disarming after reap"
        )
    )]
    pub(crate) fn record_root_reaped(&mut self) {
        #[cfg(unix)]
        self.process_group.record_root_reaped();
    }

    pub(crate) fn classify(
        &mut self,
        termination: hoimin_core::ProcessTermination,
    ) -> Result<hoimin_core::ProcessTermination, ResourceError> {
        if self
            .classification_failures
            .try_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(ResourceError::io(
                "classify portable process",
                io::Error::other("injected portable classification failure"),
            ));
        }
        Ok(termination)
    }

    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
        #[cfg(unix)]
        {
            let pid = child.id().ok_or(ResourceError::MissingProcessId)?;
            self.process_group = ProcessGroup::Signalable(
                i32::try_from(pid)
                    .map_err(|_| ResourceError::InvalidLimit("portable process group id"))?,
            );
        }
        #[cfg(windows)]
        {
            let suspended = super::suspended::SuspendedChild::open(child)?;
            self.assign_suspended(&suspended)?;
            suspended.resume()?;
        }
        #[cfg(not(any(unix, windows)))]
        let _ = child.id().ok_or(ResourceError::MissingProcessId)?;
        Ok(())
    }

    #[cfg(windows)]
    fn assign_suspended(
        &self,
        suspended: &super::suspended::SuspendedChild,
    ) -> Result<(), ResourceError> {
        #[cfg(test)]
        if self.attach_fault == AttachFault::AssignAfterDelay {
            std::thread::sleep(std::time::Duration::from_millis(200));
            return suspended.assign(std::ptr::null_mut(), "assign spawned process to job");
        }
        suspended.assign(self.job as _, "assign spawned process to job")
    }

    pub(crate) fn terminate(&mut self, live_root_owned: bool) -> Result<bool, ResourceError> {
        if self.terminated {
            return Ok(self.tree_quiescent);
        }
        #[cfg(unix)]
        let reaped_root_quiescent = if live_root_owned {
            false
        } else {
            self.process_group
                .probe_after_root_reap(process_group_is_absent)?
        };
        #[cfg(not(unix))]
        let _ = live_root_owned;
        if self
            .termination_failures
            .try_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(ResourceError::io(
                "terminate portable supervisor",
                io::Error::other("injected portable termination failure"),
            ));
        }
        #[cfg(unix)]
        if live_root_owned
            && let ProcessGroup::Signalable(group) = self.process_group
            && let Err(error) = kill_process_group(group, libc::SIGKILL)
            && error.raw_os_error() != Some(libc::ESRCH)
        {
            return Err(ResourceError::io("terminate process group", error));
        }
        #[cfg(windows)]
        terminate_job(self.job)?;
        self.terminated = true;
        #[cfg(unix)]
        {
            self.tree_quiescent = reaped_root_quiescent;
        }
        #[cfg(not(unix))]
        {
            self.tree_quiescent = true;
        }
        Ok(self.tree_quiescent)
    }

    #[cfg_attr(
        not(unix),
        allow(
            clippy::unnecessary_wraps,
            reason = "the shared supervisor API is fallible on Unix but infallible on Windows"
        )
    )]
    pub(crate) fn refresh_tree_quiescence_after_root_reap(
        &mut self,
    ) -> Result<bool, ResourceError> {
        if self.tree_quiescent {
            return Ok(true);
        }
        #[cfg(unix)]
        {
            self.tree_quiescent = self
                .process_group
                .probe_after_root_reap(process_group_is_absent)?;
        }
        #[cfg(not(unix))]
        {
            self.tree_quiescent = true;
        }
        Ok(self.tree_quiescent)
    }
}

#[cfg(unix)]
fn kill_process_group(group: i32, signal: i32) -> io::Result<()> {
    #[cfg(test)]
    if let Some(result) = unix_state_tests::intercept_kill(group, signal) {
        return result;
    }
    // SAFETY: the caller owns signal authority for this group, or uses signal zero
    // to check existence without delivering a signal. A negative PID targets the group.
    if unsafe { libc::kill(-group, signal) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(test, unix))]
#[path = "portable/unix_state_tests.rs"]
mod unix_state_tests;

#[cfg(unix)]
fn process_group_is_absent(group: i32) -> Result<bool, ResourceError> {
    let Err(error) = kill_process_group(group, 0) else {
        return Ok(false);
    };
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(true),
        Some(libc::EPERM) => Ok(false),
        _ => Err(ResourceError::io("probe portable process group", error)),
    }
}

impl Drop for PortableSupervisor {
    fn drop(&mut self) {
        let _ = self.terminate(true);
        #[cfg(windows)]
        close_job(self.job);
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use std::mem::MaybeUninit;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::time::Duration;

    use hoimin_core::{EffectId, ProcessLimits};
    use tokio::io::{AsyncRead, AsyncReadExt};
    use tokio::process::{Child, Command};

    use super::{PortableBackend, ProcessGroup};
    use crate::process::wait_after_termination;
    use crate::resource::ProcessSupervisor;

    const EXPECTED_CPU_SOFT: &str = "HOIMIN_TEST_EXPECTED_CPU_SOFT";
    const EXPECTED_CPU_HARD: &str = "HOIMIN_TEST_EXPECTED_CPU_HARD";

    #[test]
    fn recording_root_reap_disarms_signaling_before_cleanup_can_yield() {
        let mut supervisor =
            super::PortableSupervisor::new(std::sync::Arc::default(), std::sync::Arc::default())
                .unwrap();
        supervisor.process_group = ProcessGroup::Signalable(41);

        supervisor.record_root_reaped();
        let group = std::mem::replace(&mut supervisor.process_group, ProcessGroup::None);
        drop(supervisor);

        assert!(matches!(group, ProcessGroup::ProbeOnly(41)));
    }

    #[test]
    fn reaped_group_is_forgotten_before_probe_error_can_escape() {
        let mut group = ProcessGroup::Signalable(41);

        let error = group
            .probe_after_root_reap(|_| {
                Err(super::ResourceError::io(
                    "injected process-group probe",
                    std::io::Error::other("injected probe failure"),
                ))
            })
            .unwrap_err();

        assert!(matches!(group, ProcessGroup::ProbeOnly(41)));
        assert!(error.to_string().contains("injected probe failure"));
    }

    #[test]
    fn reaped_group_is_retained_until_absence_is_proven() {
        let mut group = ProcessGroup::Signalable(41);

        let present = group.probe_after_root_reap(|_| Ok(false)).unwrap();

        assert!(!present);
        assert!(matches!(group, ProcessGroup::ProbeOnly(41)));
    }

    #[test]
    fn proven_absence_discards_the_probe_identifier() {
        let mut group = ProcessGroup::ProbeOnly(41);

        let absent = group.probe_after_root_reap(|_| Ok(true)).unwrap();

        assert!(absent);
        assert!(matches!(group, ProcessGroup::None));
    }

    async fn read_pipe<R>(mut pipe: R) -> Vec<u8>
    where
        R: AsyncRead + Unpin,
    {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).await.unwrap();
        bytes
    }

    async fn finish_pipe(
        mut task: tokio::task::JoinHandle<Vec<u8>>,
        label: &str,
        errors: &mut Vec<String>,
    ) -> Vec<u8> {
        match tokio::time::timeout(Duration::from_secs(2), &mut task).await {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(error)) => {
                errors.push(format!("join {label}: {error}"));
                Vec::new()
            }
            Err(_) => {
                task.abort();
                let _ = task.await;
                errors.push(format!("read {label}: timed out and aborted"));
                Vec::new()
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn fixture_memory_limit() -> u64 {
        1024 * 1024 * 1024
    }

    #[cfg(not(target_os = "macos"))]
    #[allow(
        clippy::unnecessary_fallible_conversions,
        clippy::useless_conversion,
        reason = "libc::rlim_t signedness and width vary across supported Unix targets"
    )]
    fn finite_rlimit_to_u64(value: libc::rlim_t) -> u64 {
        u64::try_from(value).expect("finite inherited RLIMIT_AS must fit u64")
    }

    #[cfg(not(target_os = "macos"))]
    fn fixture_memory_limit() -> u64 {
        let mut inherited = MaybeUninit::<libc::rlimit>::uninit();
        // SAFETY: `inherited` points to writable storage for one rlimit value.
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_AS, inherited.as_mut_ptr()) },
            0
        );
        // SAFETY: getrlimit succeeded and initialized the value.
        let inherited = unsafe { inherited.assume_init() };
        let hard = if inherited.rlim_max == libc::RLIM_INFINITY {
            1024 * 1024 * 1024
        } else {
            finite_rlimit_to_u64(inherited.rlim_max)
        };
        assert!(
            hard >= 512 * 1024 * 1024,
            "inherited RLIMIT_AS has no safe fixture headroom"
        );
        hard.min(1024 * 1024 * 1024)
    }

    async fn cleanup_probe(
        supervisor: &mut ProcessSupervisor,
        child: &mut Child,
        terminate_group: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        if terminate_group {
            if let Err(error) = supervisor.terminate(true) {
                errors.push(format!("terminate probe group: {error}"));
                if let Err(error) = child.start_kill() {
                    errors.push(format!("kill probe root: {error}"));
                }
            }
        } else if let Err(error) = child.start_kill() {
            errors.push(format!("kill unattached probe root: {error}"));
        }
        if let Err(error) = wait_after_termination(EffectId(900), child).await {
            errors.push(format!("reap probe root: {error:?}"));
        }
        if child.id().is_none()
            && let Err(error) = supervisor.terminate(false)
        {
            errors.push(format!("disarm reaped probe supervisor: {error}"));
        }
        errors
    }

    #[test]
    #[ignore = "subprocess fixture for RLIMIT_CPU inheritance"]
    fn rlimit_cpu_probe_fixture() {
        let expected_soft = std::env::var(EXPECTED_CPU_SOFT)
            .unwrap()
            .parse::<libc::rlim_t>()
            .unwrap();
        let expected_hard = std::env::var(EXPECTED_CPU_HARD)
            .unwrap()
            .parse::<libc::rlim_t>()
            .unwrap();
        let mut observed = MaybeUninit::<libc::rlimit>::uninit();
        // SAFETY: `observed` points to writable storage for one rlimit value.
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_CPU, observed.as_mut_ptr()) },
            0
        );
        // SAFETY: getrlimit succeeded and initialized the value.
        let observed = unsafe { observed.assume_init() };

        assert_eq!(observed.rlim_cur, expected_soft);
        assert_eq!(observed.rlim_max, expected_hard);
    }

    fn requested_cpu_hard_limit(inherited: libc::rlim_t) -> libc::rlim_t {
        if inherited == libc::RLIM_INFINITY {
            31
        } else {
            inherited.min(31)
        }
    }

    fn install_fixture_cpu_limit(
        command: &mut Command,
        requested_soft: libc::rlim_t,
        requested_hard: libc::rlim_t,
    ) {
        // SAFETY: the closure changes only the soon-to-exec child and calls libc setrlimit.
        unsafe {
            command.as_std_mut().pre_exec(move || {
                let requested = libc::rlimit {
                    rlim_cur: requested_soft,
                    rlim_max: requested_hard,
                };
                if libc::setrlimit(libc::RLIMIT_CPU, std::ptr::addr_of!(requested)) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    async fn complete_rlimit_probe(
        supervisor: &mut ProcessSupervisor,
        child: &mut Child,
        stdout_task: tokio::task::JoinHandle<Vec<u8>>,
        stderr_task: tokio::task::JoinHandle<Vec<u8>>,
    ) -> (
        Result<std::process::ExitStatus, String>,
        Vec<String>,
        Vec<u8>,
        Vec<u8>,
    ) {
        if let Err(error) = supervisor.attach(child) {
            let mut cleanup = cleanup_probe(supervisor, child, false).await;
            let stdout = finish_pipe(stdout_task, "probe stdout", &mut cleanup).await;
            let stderr = finish_pipe(stderr_task, "probe stderr", &mut cleanup).await;
            return (
                Err(format!("attach RLIMIT_CPU probe: {error}")),
                cleanup,
                stdout,
                stderr,
            );
        }
        let mut diagnostics = Vec::new();
        let outcome = match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
            Ok(Ok(status)) => supervisor
                .terminate(false)
                .map(|_| status)
                .map_err(|error| format!("disarm RLIMIT_CPU supervisor: {error}")),
            Ok(Err(error)) => {
                diagnostics.extend(cleanup_probe(supervisor, child, true).await);
                Err(format!("wait for RLIMIT_CPU probe: {error}"))
            }
            Err(_) => {
                diagnostics.extend(cleanup_probe(supervisor, child, true).await);
                Err("RLIMIT_CPU probe timed out".to_owned())
            }
        };
        let stdout = finish_pipe(stdout_task, "probe stdout", &mut diagnostics).await;
        let stderr = finish_pipe(stderr_task, "probe stderr", &mut diagnostics).await;
        (outcome, diagnostics, stdout, stderr)
    }

    #[tokio::test]
    async fn portable_setup_preserves_inherited_rlimit_cpu() {
        let mut inherited = MaybeUninit::<libc::rlimit>::uninit();
        // SAFETY: `inherited` points to writable storage for one rlimit value.
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_CPU, inherited.as_mut_ptr()) },
            0
        );
        // SAFETY: getrlimit succeeded and initialized the value.
        let inherited = unsafe { inherited.assume_init() };
        let requested_hard = requested_cpu_hard_limit(inherited.rlim_max);
        assert!(
            requested_hard >= 10,
            "inherited RLIMIT_CPU has no safe fixture headroom"
        );
        let requested_soft = requested_hard - 1;

        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "resource::portable::unix_tests::rlimit_cpu_probe_fixture",
                "--test-threads=1",
            ])
            .env(EXPECTED_CPU_SOFT, requested_soft.to_string())
            .env(EXPECTED_CPU_HARD, requested_hard.to_string())
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        install_fixture_cpu_limit(&mut command, requested_soft, requested_hard);

        let backend = PortableBackend::for_tests();
        let mut supervisor = backend
            .prepare(
                &mut command,
                ProcessLimits {
                    timeout: Duration::from_secs(5),
                    max_output_bytes: 64,
                    max_memory_bytes: fixture_memory_limit(),
                    max_processes: 8,
                },
            )
            .unwrap();
        let mut child = command.spawn().unwrap();
        let stdout_task = tokio::spawn(read_pipe(child.stdout.take().unwrap()));
        let stderr_task = tokio::spawn(read_pipe(child.stderr.take().unwrap()));
        let (outcome, diagnostics, stdout, stderr) =
            complete_rlimit_probe(&mut supervisor, &mut child, stdout_task, stderr_task).await;
        let status = outcome.unwrap_or_else(|error| {
            panic!(
                "{error}; diagnostics={diagnostics:?}; stdout={} stderr={}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr),
            )
        });
        assert!(diagnostics.is_empty(), "probe diagnostics: {diagnostics:?}");
        assert!(
            status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
        );
    }

    struct ProcessGroupCleanup(i32);

    impl Drop for ProcessGroupCleanup {
        fn drop(&mut self) {
            // SAFETY: the test created this process group and uses this single cleanup signal.
            unsafe {
                libc::kill(-self.0, libc::SIGKILL);
            }
        }
    }

    #[tokio::test]
    async fn reaped_root_does_not_kill_its_live_process_group() {
        let temporary = tempfile::tempdir().unwrap();
        let marker = temporary.path().join("descendant-survived");
        let backend = PortableBackend::for_tests();
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "(sleep 0.1; touch \"$1\"; sleep 60) & exit",
                "portable-supervisor-test",
            ])
            .arg(&marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut supervisor = backend
            .prepare(
                &mut command,
                ProcessLimits {
                    timeout: Duration::from_secs(5),
                    max_output_bytes: 64,
                    max_memory_bytes: 256 * 1024 * 1024,
                    max_processes: 8,
                },
            )
            .unwrap();
        let mut child = command.spawn().unwrap();
        let group = i32::try_from(child.id().unwrap()).unwrap();
        let _cleanup = ProcessGroupCleanup(group);

        supervisor.attach(&child).unwrap();
        child.wait().await.unwrap();
        assert_eq!(child.id(), None);

        assert!(!supervisor.terminate(false).unwrap());

        tokio::time::timeout(Duration::from_secs(1), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("descendant must survive cleanup after the root is reaped");
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "The shared command configuration API propagates platform setup failures."
)]
fn configure_command(command: &mut Command, limits: ProcessLimits) -> Result<(), ResourceError> {
    use std::os::unix::process::CommandExt;

    let memory = limits.max_memory_bytes;
    // SAFETY: this retains the existing pre-exec setup contract: the closure mutates only
    // child-local process-group and address-space limits before exec.
    unsafe {
        command.as_std_mut().pre_exec(move || {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            let address_space = libc::rlimit {
                rlim_cur: memory as libc::rlim_t,
                rlim_max: memory as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_AS, std::ptr::addr_of!(address_space)) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the shared command configuration API retains a fallible signature across target-specific implementations"
)]
fn configure_command(command: &mut Command, _limits: ProcessLimits) -> Result<(), ResourceError> {
    use std::os::unix::process::CommandExt;

    // SAFETY: this closure changes only the soon-to-exec child process.
    unsafe {
        command.as_std_mut().pre_exec(move || {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the shared command configuration API retains a fallible signature across target-specific implementations"
)]
fn configure_command(command: &mut Command, _limits: ProcessLimits) -> Result<(), ResourceError> {
    super::suspended::configure(command);
    Ok(())
}

#[cfg(not(any(unix, windows)))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the unsupported-platform no-op retains the shared fallible platform configuration interface"
)]
fn configure_command(_command: &mut Command, _limits: ProcessLimits) -> Result<(), ResourceError> {
    Ok(())
}

#[cfg(windows)]
fn create_kill_on_close_job() -> Result<isize, ResourceError> {
    use std::mem::{size_of, zeroed};
    use std::ptr::null;
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    // SAFETY: pointers reference initialized local storage for the documented Win32 call.
    unsafe {
        let job = CreateJobObjectW(null(), null());
        if job.is_null() {
            return Err(ResourceError::io(
                "create process job",
                io::Error::last_os_error(),
            ));
        }
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const information).cast(),
            u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                .expect("Windows Job Object structure size fits u32"),
        ) == 0
        {
            let error = io::Error::last_os_error();
            windows_sys::Win32::Foundation::CloseHandle(job);
            return Err(ResourceError::io("configure process job", error));
        }
        Ok(job as isize)
    }
}

#[cfg(windows)]
fn terminate_job(job: isize) -> Result<(), ResourceError> {
    use windows_sys::Win32::System::JobObjects::TerminateJobObject;

    // SAFETY: job is a live handle owned by this supervisor.
    if unsafe { TerminateJobObject(job as _, 1) } == 0 {
        return Err(ResourceError::io(
            "terminate process job",
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn close_job(job: isize) {
    // SAFETY: job is closed exactly once by the supervisor Drop implementation.
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(job as _);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use std::time::{Duration, Instant};

    use camino::{Utf8Path, Utf8PathBuf};
    use hoimin_core::{CommandArg, EffectFailure, EffectId, ProcessLimits, RunProcess};
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    };

    use super::PortableBackend;
    use crate::process::ProcessHandler;
    use crate::resource::ResourceBackend;

    fn arg(value: impl AsRef<OsStr>) -> CommandArg {
        CommandArg::Windows(value.as_ref().encode_wide().collect())
    }

    fn python() -> CommandArg {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let virtualenv = workspace.join(".venv/Scripts/python.exe");
        if virtualenv.is_file() {
            arg(virtualenv)
        } else {
            arg("python")
        }
    }

    fn terminate_fixture(pid_path: &std::path::Path) {
        let deadline = Instant::now() + Duration::from_millis(500);
        let pid = loop {
            if let Ok(pid) = fs::read_to_string(pid_path) {
                break pid
                    .parse()
                    .expect("detached fixture PID must be an unsigned integer");
            }
            if Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        // SAFETY: the PID names the test fixture, access is limited to termination and waiting,
        // and the checked handle is closed after the process reaches a terminal state.
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
            assert!(!process.is_null(), "detached fixture process must open");
            let terminated = TerminateProcess(process, 1);
            let termination_error = (terminated == 0).then(std::io::Error::last_os_error);
            let wait_result = WaitForSingleObject(process, 1_000);
            let close_result = CloseHandle(process);

            assert_ne!(
                terminated,
                0,
                "detached fixture termination must succeed: {}",
                termination_error.expect("failed termination records its error")
            );
            assert_eq!(
                wait_result, WAIT_OBJECT_0,
                "detached fixture must terminate within one second"
            );
            assert_ne!(close_result, 0, "fixture handle must close");
        }
    }

    #[tokio::test]
    async fn attach_failure_prevents_immediate_detached_descendant() {
        for sequence in 0..4 {
            let temporary = tempfile::tempdir().unwrap();
            let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
            let root_marker = output_dir.join(format!("root-{sequence}.marker"));
            let descendant_marker = output_dir.join(format!("descendant-{sequence}.marker"));
            let descendant_pid = output_dir.join(format!("descendant-{sequence}.pid"));
            let handler = ProcessHandler::new(
                ResourceBackend::Portable(PortableBackend::with_delayed_assignment_failure()),
                output_dir.to_owned(),
            );
            let code = "import pathlib,subprocess,sys,time; pathlib.Path(sys.argv[1]).write_text('ran'); child=subprocess.Popen([sys.executable,'-c','import pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(\"escaped\"); time.sleep(30)',sys.argv[2]], creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP); pathlib.Path(sys.argv[3]).write_text(str(child.pid)); time.sleep(30)";
            let request = RunProcess {
                id: EffectId(300 + sequence),
                context: hoimin_core::ProbeContext,
                argv: vec![
                    python(),
                    arg("-c"),
                    arg(code),
                    arg(root_marker.as_std_path()),
                    arg(descendant_marker.as_std_path()),
                    arg(descendant_pid.as_std_path()),
                ],
                cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
                limits: ProcessLimits {
                    timeout: Duration::from_secs(10),
                    max_output_bytes: 64,
                    max_memory_bytes: 256 * 1024 * 1024,
                    max_processes: 8,
                },
            };

            let outcome =
                tokio::time::timeout(Duration::from_secs(2), handler.handle(request)).await;
            terminate_fixture(descendant_pid.as_std_path());
            let error = outcome
                .expect("attach failure cleanup is bounded")
                .expect_err("delayed assignment failure is returned");

            assert!(matches!(
                error.failure,
                EffectFailure::Io { ref code, .. } if code == "process.resource.attach"
            ));
            assert!(
                !root_marker.exists(),
                "suspended root code must not execute"
            );
            assert!(
                !descendant_marker.exists(),
                "a pre-assignment descendant must not escape the portable job"
            );
        }
    }
}
