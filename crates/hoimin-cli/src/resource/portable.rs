use std::io;

use hoimin_core::{ProcessLimits, ResourceMode};
use tokio::process::{Child, Command};

use super::{ProcessSupervisor, ResourceError};

#[derive(Clone, Debug, Default)]
pub struct PortableBackend {
    diagnostic: Option<String>,
}

impl PortableBackend {
    /// # Errors
    ///
    /// Returns an error on Linux when best-effort memory limiting was not explicitly allowed.
    pub fn new(allow_best_effort_memory: bool) -> Result<Self, ResourceError> {
        #[cfg(target_os = "linux")]
        if !allow_best_effort_memory {
            return Err(ResourceError::BestEffortNotAllowed(
                "portable Linux uses per-process RLIMIT_AS/RLIMIT_CPU and process groups".into(),
            ));
        }
        #[cfg(not(target_os = "linux"))]
        let _ = allow_best_effort_memory;
        Ok(Self { diagnostic: None })
    }

    #[must_use]
    pub fn for_tests() -> Self {
        Self::default()
    }

    pub(crate) fn with_diagnostic(diagnostic: String) -> Self {
        Self {
            diagnostic: Some(diagnostic),
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
        let _ = self;
        configure_command(command, limits)?;
        Ok(ProcessSupervisor::Portable(PortableSupervisor::new()?))
    }
}

#[derive(Debug)]
pub(crate) struct PortableSupervisor {
    #[cfg(unix)]
    process_group: Option<i32>,
    #[cfg(windows)]
    job: isize,
    terminated: bool,
}

impl PortableSupervisor {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "Windows job-object creation is fallible while Unix construction is not."
    )]
    fn new() -> Result<Self, ResourceError> {
        #[cfg(windows)]
        let job = create_kill_on_close_job()?;
        Ok(Self {
            #[cfg(unix)]
            process_group: None,
            #[cfg(windows)]
            job,
            terminated: false,
        })
    }

    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
        let pid = child.id().ok_or(ResourceError::MissingProcessId)?;
        #[cfg(unix)]
        {
            self.process_group = Some(
                i32::try_from(pid)
                    .map_err(|_| ResourceError::InvalidLimit("portable process group id"))?,
            );
        }
        #[cfg(windows)]
        // The child can execute between spawn and this assignment. Task 8 closes that known
        // pre-assignment race by adding suspended startup before hard-limit configuration.
        assign_to_job(self.job, pid)?;
        #[cfg(not(any(unix, windows)))]
        let _ = pid;
        Ok(())
    }

    pub(crate) fn terminate(&mut self) -> Result<(), ResourceError> {
        if self.terminated {
            return Ok(());
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

#[cfg(unix)]
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

#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the non-Unix no-op retains the shared fallible platform configuration interface"
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
fn assign_to_job(job: isize, pid: u32) -> Result<(), ResourceError> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    // SAFETY: the process handle is checked and closed after assignment.
    unsafe {
        let process = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE,
            0,
            pid,
        );
        if process.is_null() {
            return Err(ResourceError::io(
                "open spawned process",
                io::Error::last_os_error(),
            ));
        }
        let assigned = AssignProcessToJobObject(job as _, process);
        let error = (assigned == 0).then(io::Error::last_os_error);
        CloseHandle(process);
        error.map_or(Ok(()), |error| {
            Err(ResourceError::io("assign spawned process to job", error))
        })
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
