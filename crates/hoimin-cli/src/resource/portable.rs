use std::io;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use hoimin_core::{ProcessLimits, ResourceMode};
use tokio::process::{Child, Command};

use super::{ProcessSupervisor, ResourceError};

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AttachFault {
    #[default]
    None,
    #[cfg(test)]
    AssignAfterDelay,
}

#[cfg(target_os = "macos")]
const MACOS_BEST_EFFORT_DIAGNOSTIC: &str =
    "macOS uses process groups and RLIMIT_CPU; max-memory is not enforced";

#[derive(Clone, Debug, Default)]
pub struct PortableBackend {
    diagnostic: Option<String>,
    termination_failures: Arc<AtomicU8>,
    #[cfg(windows)]
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
                    "portable Linux uses per-process RLIMIT_AS/RLIMIT_CPU and process groups"
                        .into(),
                ));
            }
            Ok(Self {
                diagnostic: None,
                termination_failures: Arc::new(AtomicU8::new(0)),
                #[cfg(windows)]
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
                termination_failures: Arc::new(AtomicU8::new(0)),
                #[cfg(windows)]
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
            termination_failures: Arc::new(AtomicU8::new(1)),
            #[cfg(windows)]
            attach_fault: AttachFault::None,
        }
    }

    pub(crate) fn with_diagnostic(diagnostic: String) -> Self {
        Self {
            diagnostic: Some(diagnostic),
            termination_failures: Arc::new(AtomicU8::new(0)),
            #[cfg(windows)]
            attach_fault: AttachFault::None,
        }
    }

    #[cfg(all(test, windows))]
    fn with_delayed_assignment_failure() -> Self {
        Self {
            diagnostic: None,
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
            Arc::clone(&self.termination_failures),
            #[cfg(windows)]
            self.attach_fault,
        )?))
    }
}

#[derive(Debug)]
pub(crate) struct PortableSupervisor {
    #[cfg(unix)]
    process_group: Option<i32>,
    #[cfg(windows)]
    job: isize,
    #[cfg(windows)]
    attach_fault: AttachFault,
    terminated: bool,
    termination_failures: Arc<AtomicU8>,
}

impl PortableSupervisor {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "Windows job-object creation is fallible while Unix construction is not."
    )]
    fn new(
        termination_failures: Arc<AtomicU8>,
        #[cfg(windows)] attach_fault: AttachFault,
    ) -> Result<Self, ResourceError> {
        #[cfg(windows)]
        let job = create_kill_on_close_job()?;
        Ok(Self {
            #[cfg(unix)]
            process_group: None,
            #[cfg(windows)]
            job,
            #[cfg(windows)]
            attach_fault,
            terminated: false,
            termination_failures,
        })
    }

    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
        #[cfg(unix)]
        {
            let pid = child.id().ok_or(ResourceError::MissingProcessId)?;
            self.process_group = Some(
                i32::try_from(pid)
                    .map_err(|_| ResourceError::InvalidLimit("portable process group id"))?,
            );
        }
        #[cfg(windows)]
        {
            let suspended = super::suspended::SuspendedChild::open(child)?;
            #[cfg(test)]
            if self.attach_fault == AttachFault::AssignAfterDelay {
                std::thread::sleep(std::time::Duration::from_millis(200));
                return Err(ResourceError::io(
                    "assign spawned process to job",
                    io::Error::other("injected delayed assignment failure"),
                ));
            }
            suspended.assign(self.job as _, "assign spawned process to job")?;
            suspended.resume()?;
        }
        #[cfg(not(any(unix, windows)))]
        let _ = child.id().ok_or(ResourceError::MissingProcessId)?;
        Ok(())
    }

    pub(crate) fn terminate(&mut self) -> Result<(), ResourceError> {
        if self.terminated {
            return Ok(());
        }
        if self
            .termination_failures
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
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
        if let Some(group) = self.process_group {
            // SAFETY: the child created this process group; negative pid targets that group only.
            let result = unsafe { libc::kill(-group, libc::SIGKILL) };
            if result != 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(ResourceError::io("terminate process group", error));
                }
            }
        }
        #[cfg(windows)]
        terminate_job(self.job)?;
        self.terminated = true;
        Ok(())
    }
}

impl Drop for PortableSupervisor {
    fn drop(&mut self) {
        let _ = self.terminate();
        #[cfg(windows)]
        close_job(self.job);
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
    let cpu_seconds = limits
        .timeout
        .as_secs()
        .saturating_add(u64::from(limits.timeout.subsec_nanos() != 0))
        .max(1);
    // SAFETY: this closure uses only async-signal-safe libc calls before exec.
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
            let cpu = libc::rlimit {
                rlim_cur: cpu_seconds as libc::rlim_t,
                rlim_max: cpu_seconds as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_CPU, std::ptr::addr_of!(cpu)) != 0 {
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
fn configure_command(command: &mut Command, limits: ProcessLimits) -> Result<(), ResourceError> {
    use std::os::unix::process::CommandExt;

    let cpu_seconds = limits
        .timeout
        .as_secs()
        .saturating_add(u64::from(limits.timeout.subsec_nanos() != 0))
        .max(1);
    // SAFETY: this closure uses only async-signal-safe libc calls before exec.
    unsafe {
        command.as_std_mut().pre_exec(move || {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            let cpu = libc::rlimit {
                rlim_cur: cpu_seconds as libc::rlim_t,
                rlim_max: cpu_seconds as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_CPU, std::ptr::addr_of!(cpu)) != 0 {
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
    use std::time::Duration;

    use camino::{Utf8Path, Utf8PathBuf};
    use hoimin_core::{CommandArg, EffectFailure, EffectId, ProcessLimits, RunProcess};
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

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
        let Ok(pid) = fs::read_to_string(pid_path) else {
            return;
        };
        let Ok(pid) = pid.parse() else {
            return;
        };
        // SAFETY: the handle is checked and closed after terminating the test fixture.
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !process.is_null() {
                TerminateProcess(process, 1);
                CloseHandle(process);
            }
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
                worker: None,
                run_id: None,
                mutant_id: None,
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

            let error = tokio::time::timeout(Duration::from_secs(2), handler.handle(request))
                .await
                .expect("attach failure cleanup is bounded")
                .expect_err("delayed assignment failure is returned");
            terminate_fixture(descendant_pid.as_std_path());

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
