mod portable;
#[cfg(windows)]
mod windows;

use hoimin_core::{ProcessLimits, ResourceMode};
use tokio::process::{Child, Command};

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
    #[error("Windows run-wide Job Object is closed")]
    RunClosed,
    #[error("invalid Windows Job Object limit: {0}")]
    InvalidLimit(&'static str),
}

impl ResourceError {
    pub(crate) fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }
}

#[derive(Clone, Debug)]
pub enum ResourceBackend {
    Portable(PortableBackend),
    #[cfg(windows)]
    Windows(WindowsBackend),
}

impl ResourceBackend {
    pub fn mode(&self) -> ResourceMode {
        match self {
            Self::Portable(backend) => backend.mode(),
            #[cfg(windows)]
            Self::Windows(backend) => backend.mode(),
        }
    }

    pub(crate) fn prepare(
        &self,
        command: &mut Command,
        limits: ProcessLimits,
    ) -> Result<ProcessSupervisor, ResourceError> {
        match self {
            Self::Portable(backend) => backend.prepare(command, limits),
            #[cfg(windows)]
            Self::Windows(backend) => backend.prepare(command, limits),
        }
    }

    pub fn close(&self) -> Result<(), ResourceError> {
        match self {
            Self::Portable(_) => Ok(()),
            #[cfg(windows)]
            Self::Windows(backend) => backend.close(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProcessSupervisor {
    Portable(portable::PortableSupervisor),
    #[cfg(windows)]
    Windows(windows::WindowsSupervisor),
}

impl ProcessSupervisor {
    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
        match self {
            Self::Portable(supervisor) => supervisor.attach(child),
            #[cfg(windows)]
            Self::Windows(supervisor) => supervisor.attach(child),
        }
    }

    pub(crate) fn terminate(&mut self) -> Result<(), ResourceError> {
        match self {
            Self::Portable(supervisor) => supervisor.terminate(),
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
            #[cfg(windows)]
            Self::Windows(supervisor) => supervisor.classify(termination),
        }
    }
}
