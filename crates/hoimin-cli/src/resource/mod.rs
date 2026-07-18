mod linux;
mod portable;
#[cfg(windows)]
mod windows;

use hoimin_core::{ProcessLimits, ResourceMode};
use std::ffi::OsString;
use tokio::process::{Child, Command};

#[cfg(target_os = "linux")]
pub use linux::probe_linux_cgroup_with_launcher;
pub use linux::{
    CgroupCapabilities, CgroupEventCounters, parse_cgroup_event_counters, probe_linux_cgroup,
    run_linux_launcher_from, select_linux_backend,
};
#[cfg(target_os = "linux")]
pub use linux::{LinuxBackend, PendingCgroupCleanup};
pub use portable::PortableBackend;
#[cfg(windows)]
pub use windows::WindowsBackend;

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("portable resource limits require --allow-best-effort-memory: {0}")]
    BestEffortNotAllowed(String),
    #[error("{operation} failed: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("spawned process did not expose a process id")]
    MissingProcessId,
    #[error("run-wide hard resource backend is closed")]
    RunClosed,
    #[error("invalid Windows Job Object limit: {0}")]
    InvalidLimit(&'static str),
    #[error("invalid cgroup event data: {0}")]
    InvalidCgroupData(String),
    #[error("retryable cgroup cleanup failed for {path}: {source}")]
    CgroupCleanup {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cgroup accounting failed: {accounting}; cleanup also failed: {cleanup}")]
    CgroupAccountingAndCleanup {
        accounting: Box<ResourceError>,
        cleanup: Box<ResourceError>,
    },
    #[error("cgroup operation failed: {operation}; cleanup also failed: {cleanup}")]
    CgroupOperationAndCleanup {
        operation: Box<ResourceError>,
        cleanup: Box<ResourceError>,
    },
    #[cfg(target_os = "linux")]
    #[error("cgroup setup cleanup remains pending: {reason}; last cleanup error: {cleanup}")]
    CgroupCleanupPending {
        reason: String,
        cleanup: Box<ResourceError>,
        pending: PendingCgroupCleanup,
    },
}

impl ResourceError {
    pub(crate) fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }
}

#[derive(Clone, Debug)]
pub enum ResourceBackend {
    Portable(PortableBackend),
    #[cfg(target_os = "linux")]
    LinuxHard(LinuxBackend),
    #[cfg(windows)]
    Windows(WindowsBackend),
}

impl ResourceBackend {
    pub fn mode(&self) -> ResourceMode {
        match self {
            Self::Portable(backend) => backend.mode(),
            #[cfg(target_os = "linux")]
            Self::LinuxHard(backend) => backend.mode(),
            #[cfg(windows)]
            Self::Windows(backend) => backend.mode(),
        }
    }

    pub fn diagnostic(&self) -> Option<&str> {
        match self {
            Self::Portable(backend) => backend.diagnostic(),
            #[cfg(target_os = "linux")]
            Self::LinuxHard(backend) => backend.diagnostics().first().map(String::as_str),
            #[cfg(windows)]
            Self::Windows(_) => None,
        }
    }

    pub(crate) fn wrap_argv(&self, argv: Vec<OsString>) -> Vec<OsString> {
        match self {
            Self::Portable(_) => argv,
            #[cfg(target_os = "linux")]
            Self::LinuxHard(backend) => backend.wrap_argv(argv),
            #[cfg(windows)]
            Self::Windows(_) => argv,
        }
    }

    pub(crate) fn prepare(
        &self,
        command: &mut Command,
        limits: ProcessLimits,
    ) -> Result<ProcessSupervisor, ResourceError> {
        match self {
            Self::Portable(backend) => backend.prepare(command, limits),
            #[cfg(target_os = "linux")]
            Self::LinuxHard(backend) => backend.prepare(command, limits),
            #[cfg(windows)]
            Self::Windows(backend) => backend.prepare(command, limits),
        }
    }

    pub fn close(&self) -> Result<(), ResourceError> {
        match self {
            Self::Portable(_) => Ok(()),
            #[cfg(target_os = "linux")]
            Self::LinuxHard(backend) => backend.close(),
            #[cfg(windows)]
            Self::Windows(backend) => backend.close(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProcessSupervisor {
    Portable(portable::PortableSupervisor),
    #[cfg(target_os = "linux")]
    Linux(linux::LinuxSupervisor),
    #[cfg(windows)]
    Windows(windows::WindowsSupervisor),
}

impl ProcessSupervisor {
    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
        match self {
            Self::Portable(supervisor) => supervisor.attach(child),
            #[cfg(target_os = "linux")]
            Self::Linux(supervisor) => supervisor.attach(child),
            #[cfg(windows)]
            Self::Windows(supervisor) => supervisor.attach(child),
        }
    }

    pub(crate) fn terminate(&mut self, live_root_owned: bool) -> Result<(), ResourceError> {
        #[cfg(not(target_os = "linux"))]
        let _ = live_root_owned;
        match self {
            Self::Portable(supervisor) => supervisor.terminate(),
            #[cfg(target_os = "linux")]
            Self::Linux(supervisor) => supervisor.terminate(live_root_owned),
            #[cfg(windows)]
            Self::Windows(supervisor) => supervisor.terminate(),
        }
    }

    pub(crate) fn classify(
        &mut self,
        termination: hoimin_core::ProcessTermination,
    ) -> Result<hoimin_core::ProcessTermination, ResourceError> {
        match self {
            Self::Portable(_) => Ok(termination),
            #[cfg(target_os = "linux")]
            Self::Linux(supervisor) => supervisor.classify(termination),
            #[cfg(windows)]
            Self::Windows(supervisor) => supervisor.classify(termination),
        }
    }
}
