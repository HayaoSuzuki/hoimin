mod portable;

use hoimin_core::{ProcessLimits, ResourceMode};
use tokio::process::{Child, Command};

pub use portable::PortableBackend;

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
}

impl ResourceError {
    pub(crate) fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }
}

#[derive(Clone, Debug)]
pub enum ResourceBackend {
    Portable(PortableBackend),
}

impl ResourceBackend {
    pub fn mode(&self) -> ResourceMode {
        match self {
            Self::Portable(backend) => backend.mode(),
        }
    }

    pub(crate) fn prepare(
        &self,
        command: &mut Command,
        limits: ProcessLimits,
    ) -> Result<ProcessSupervisor, ResourceError> {
        match self {
            Self::Portable(backend) => backend.prepare(command, limits),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProcessSupervisor {
    Portable(portable::PortableSupervisor),
}

impl ProcessSupervisor {
    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
        match self {
            Self::Portable(supervisor) => supervisor.attach(child),
        }
    }

    pub(crate) fn terminate(&mut self) -> Result<(), ResourceError> {
        match self {
            Self::Portable(supervisor) => supervisor.terminate(),
        }
    }
}
