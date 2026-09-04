use std::collections::HashSet;
use std::ffi::c_void;
use std::io;
use std::mem::{size_of, zeroed};
use std::ptr::{dangling_mut, null, null_mut};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use hoimin_core::{ProcessLimits, ProcessTermination, ResourceMode, RunLimits};
use tokio::process::{Child, Command};
use uuid::Uuid;
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus};
use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_ASSOCIATE_COMPLETION_PORT,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectAssociateCompletionPortInformation, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject,
};
use windows_sys::Win32::System::SystemServices::{
    JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS, JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT,
    JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO, JOB_OBJECT_MSG_EXIT_PROCESS,
    JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

use super::{ProcessSupervisor, ResourceError};

const MEMORY_VIOLATION: u8 = 1;
const PROCESS_VIOLATION: u8 = 2;
const NOTIFICATION_BARRIER_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct WindowsBackend {
    inner: Arc<WindowsRunJob>,
    attach_fault: AttachFault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachFault {
    None,
    #[cfg(test)]
    Assign,
    #[cfg(test)]
    NestedAssign,
    #[cfg(test)]
    Resume,
    #[cfg(test)]
    ExitHandleOpen,
}

impl WindowsBackend {
    /// # Errors
    ///
    /// Returns an error when configured limits cannot be represented by the Windows Job Object API
    /// or its run-wide job cannot be created.
    pub fn new(_limits: &RunLimits) -> Result<Self, ResourceError> {
        Ok(Self {
            inner: Arc::new(WindowsRunJob::new()?),
            attach_fault: AttachFault::None,
        })
    }

    #[cfg(test)]
    fn with_test_fault(
        limits: &RunLimits,
        attach_fault: AttachFault,
    ) -> Result<Self, ResourceError> {
        let mut backend = Self::new(limits)?;
        backend.attach_fault = attach_fault;
        Ok(backend)
    }

    #[cfg(test)]
    fn with_test_close_failure(limits: &RunLimits) -> Result<Self, ResourceError> {
        let backend = Self::new(limits)?;
        backend.inner.close_failures.store(1, Ordering::Release);
        Ok(backend)
    }

    #[must_use]
    pub fn mode(&self) -> ResourceMode {
        ResourceMode::Hard
    }

    /// # Errors
    ///
    /// Returns an error when terminating the run-wide Job Object fails.
    pub fn close(&self) -> Result<(), ResourceError> {
        self.inner.close()
    }

    pub(crate) fn prepare(
        &self,
        command: &mut Command,
        limits: ProcessLimits,
    ) -> Result<ProcessSupervisor, ResourceError> {
        {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.inner.drain_pending(&mut state)?;
            if state.closed {
                return Err(ResourceError::RunClosed);
            }
        }
        super::suspended::configure(command);
        let root_job = create_limited_root_job(limits)?;
        let completion_port = create_completion_port()?;
        associate_completion_port(root_job.raw(), completion_port.raw())?;
        Ok(ProcessSupervisor::Windows(WindowsSupervisor {
            run: Arc::clone(&self.inner),
            root_job,
            completion_port,
            root_id: Uuid::new_v4(),
            signal: Arc::new(RootSignal::default()),
            pid: None,
            terminated: false,
            attach_fault: self.attach_fault,
        }))
    }
}

#[derive(Debug)]
struct WindowsRunJob {
    job: OwnedHandle,
    completion_port: OwnedHandle,
    state: Mutex<RunState>,
    #[cfg(test)]
    close_failures: AtomicU8,
}

#[derive(Debug, Default)]
struct RunState {
    closed: bool,
    terminated: bool,
    active: Vec<ActiveRoot>,
    exited_roots: HashSet<Uuid>,
}

#[derive(Debug)]
struct ActiveRoot {
    id: Uuid,
    pid: u32,
    signal: Option<Weak<RootSignal>>,
    // Keeping the process object open prevents Windows from recycling its PID before the
    // corresponding Job Object exit notification has been consumed.
    process: Option<OwnedHandle>,
}

impl Drop for ActiveRoot {
    fn drop(&mut self) {
        // Make the PID-reuse barrier explicit: removing the registration closes its handle.
        drop(self.process.take());
    }
}

#[derive(Debug, Default)]
struct RootSignal {
    violations: AtomicU8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootExitBarrier {
    NotificationConsumed,
    ProcessSignaled,
}

impl WindowsRunJob {
    fn new() -> Result<Self, ResourceError> {
        let job = create_job()?;
        configure_run_job(job.raw())?;
        let completion_port = create_completion_port()?;
        associate_completion_port(job.raw(), completion_port.raw())?;
        Ok(Self {
            job,
            completion_port,
            state: Mutex::new(RunState::default()),
            #[cfg(test)]
            close_failures: AtomicU8::new(0),
        })
    }

    fn close(&self) -> Result<(), ResourceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.terminated {
            return Ok(());
        }
        state.closed = true;
        #[cfg(test)]
        if self.close_failures.swap(0, Ordering::AcqRel) != 0 {
            return Err(ResourceError::io(
                "terminate run-wide process job",
                io::Error::other("injected run termination failure"),
            ));
        }
        terminate_job(self.job.raw(), "terminate run-wide process job")?;
        state.terminated = true;
        Ok(())
    }

    fn attach_root(
        &self,
        root_job: HANDLE,
        root_id: Uuid,
        signal: &Arc<RootSignal>,
        child: &Child,
        attach_fault: AttachFault,
    ) -> Result<u32, ResourceError> {
        #[cfg(not(test))]
        let _ = attach_fault;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.drain_pending(&mut state)?;
        if state.closed {
            return Err(ResourceError::RunClosed);
        }
        #[cfg(test)]
        if attach_fault == AttachFault::Assign {
            return Err(ResourceError::io(
                "assign process to run-wide job",
                io::Error::other("injected assignment failure"),
            ));
        }
        let child = super::suspended::SuspendedChild::open(child)?;
        let pid = child.pid();
        child.assign(self.job.raw(), "assign process to run-wide job")?;
        #[cfg(test)]
        if attach_fault == AttachFault::NestedAssign {
            register_root(&mut state, root_id, pid, None, child, attach_fault)?;
            return Err(ResourceError::io(
                "assign process to nested root job",
                io::Error::other("injected nested assignment failure"),
            ));
        }
        if let Err(error) = child.assign(root_job, "assign process to nested root job") {
            register_root(&mut state, root_id, pid, None, child, attach_fault)?;
            return Err(error);
        }
        #[cfg(test)]
        if attach_fault == AttachFault::Resume {
            register_root(&mut state, root_id, pid, None, child, attach_fault)?;
            return Err(ResourceError::io(
                "resume suspended primary thread",
                io::Error::other("injected resume failure"),
            ));
        }
        if let Err(error) = child.resume() {
            register_root(&mut state, root_id, pid, None, child, attach_fault)?;
            return Err(error);
        }
        register_root(&mut state, root_id, pid, Some(signal), child, attach_fault)?;
        Ok(pid)
    }

    fn classify_root(
        &self,
        root_id: Uuid,
        signal: &RootSignal,
        termination: ProcessTermination,
    ) -> Result<ProcessTermination, ResourceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match self.drain_until_root_exit(&mut state, root_id)? {
            RootExitBarrier::NotificationConsumed => {
                forget_root_generation(&mut state, root_id);
            }
            RootExitBarrier::ProcessSignaled => {
                detach_root_generation(&mut state, root_id);
            }
        }
        let violations = signal.violations.load(Ordering::Acquire);
        Ok(if violations & MEMORY_VIOLATION != 0 {
            ProcessTermination::OutOfMemory
        } else if violations & PROCESS_VIOLATION != 0 {
            ProcessTermination::ProcessLimit
        } else {
            termination
        })
    }

    fn unregister_root(&self, root_id: Uuid) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        detach_root_generation(&mut state, root_id);
    }

    fn drain_pending(&self, state: &mut RunState) -> Result<(), ResourceError> {
        loop {
            match self.next_notification(0)? {
                Some((message, pid)) => self.record_notification(state, message, pid)?,
                None => return Ok(()),
            }
        }
    }

    fn drain_until_root_exit(
        &self,
        state: &mut RunState,
        root_id: Uuid,
    ) -> Result<RootExitBarrier, ResourceError> {
        if root_notification_consumed(state, root_id) {
            return Ok(RootExitBarrier::NotificationConsumed);
        }
        let deadline = Instant::now() + NOTIFICATION_BARRIER_TIMEOUT;
        loop {
            while let Some((message, pid)) = self.next_notification(0)? {
                self.record_notification(state, message, pid)?;
                if root_notification_consumed(state, root_id) {
                    return Ok(RootExitBarrier::NotificationConsumed);
                }
            }
            if root_process_signaled(state, root_id)? {
                return Ok(RootExitBarrier::ProcessSignaled);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ResourceError::io(
                    "wait for Job Object exit notification",
                    io::Error::new(io::ErrorKind::TimedOut, "root exit notification timed out"),
                ));
            }
            let timeout_ms = u32::try_from(remaining.as_millis().min(u128::from(u32::MAX)))
                .expect("bounded timeout fits u32");
            if let Some((message, pid)) = self.next_notification(timeout_ms.max(1))? {
                self.record_notification(state, message, pid)?;
                if root_notification_consumed(state, root_id) {
                    return Ok(RootExitBarrier::NotificationConsumed);
                }
            } else {
                if root_process_signaled(state, root_id)? {
                    return Ok(RootExitBarrier::ProcessSignaled);
                }
                return Err(ResourceError::io(
                    "wait for Job Object exit notification",
                    io::Error::new(io::ErrorKind::TimedOut, "root exit notification timed out"),
                ));
            }
        }
    }

    fn next_notification(&self, timeout_ms: u32) -> Result<Option<(u32, u32)>, ResourceError> {
        Self::next_notification_from(self.completion_port.raw(), timeout_ms)
    }

    fn next_notification_from(
        port: HANDLE,
        timeout_ms: u32,
    ) -> Result<Option<(u32, u32)>, ResourceError> {
        let mut message = 0_u32;
        let mut key = 0_usize;
        let mut overlapped = null_mut();
        // SAFETY: all pointers reference live local storage and the completion port is owned.
        let ok = unsafe {
            GetQueuedCompletionStatus(
                port,
                &raw mut message,
                &raw mut key,
                &raw mut overlapped,
                timeout_ms,
            )
        };
        if ok != 0 {
            let pid = u32::try_from(overlapped as usize)
                .map_err(|_| ResourceError::InvalidLimit("Job Object notification process id"))?;
            return Ok(Some((message, pid)));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(i32::try_from(WAIT_TIMEOUT).expect("WAIT_TIMEOUT fits i32"))
        {
            Ok(None)
        } else {
            Err(ResourceError::io("read Job Object notification", error))
        }
    }

    fn record_notification(
        &self,
        state: &mut RunState,
        message: u32,
        pid: u32,
    ) -> Result<(), ResourceError> {
        let job_is_empty = message == JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO
            && active_process_count(self.job.raw())? == 0;
        record_notification(state, message, pid, job_is_empty);
        Ok(())
    }
}

fn record_notification(state: &mut RunState, message: u32, pid: u32, job_is_empty: bool) {
    match message {
        JOB_OBJECT_MSG_JOB_MEMORY_LIMIT => mark_active(state, MEMORY_VIOLATION),
        JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT => mark_active(state, PROCESS_VIOLATION),
        JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO if job_is_empty => {
            state.exited_roots.extend(
                state
                    .active
                    .drain(..)
                    .filter(|root| root.signal.is_some())
                    .map(|root| root.id),
            );
        }
        JOB_OBJECT_MSG_EXIT_PROCESS | JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS => {
            if let Some(index) = state.active.iter().position(|root| root.pid == pid) {
                let root = state.active.remove(index);
                if root.signal.is_some() {
                    state.exited_roots.insert(root.id);
                }
            }
        }
        _ => {}
    }
}

fn root_notification_consumed(state: &RunState, root_id: Uuid) -> bool {
    state.exited_roots.contains(&root_id) || state.active.iter().all(|root| root.id != root_id)
}

fn root_process_signaled(state: &RunState, root_id: Uuid) -> Result<bool, ResourceError> {
    let Some(process) = state
        .active
        .iter()
        .find(|root| root.id == root_id)
        .and_then(|root| root.process.as_ref())
    else {
        return Ok(false);
    };
    // SAFETY: the generation retains ownership of this process handle while it is registered.
    match unsafe { WaitForSingleObject(process.raw(), 0) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(ResourceError::io(
            "wait for root process exit",
            io::Error::last_os_error(),
        )),
    }
}

fn register_root(
    state: &mut RunState,
    root_id: Uuid,
    pid: u32,
    signal: Option<&Arc<RootSignal>>,
    child: super::suspended::SuspendedChild,
    attach_fault: AttachFault,
) -> Result<(), ResourceError> {
    let original_process = child.into_process_handle();
    let process = match open_root_exit_handle(pid, attach_fault) {
        Ok(process) => process,
        Err(error) => {
            state.active.push(ActiveRoot {
                id: root_id,
                pid,
                signal: None,
                process: Some(original_process),
            });
            return Err(error);
        }
    };
    state.active.push(ActiveRoot {
        id: root_id,
        pid,
        signal: signal.map(Arc::downgrade),
        process: Some(process),
    });
    drop(original_process);
    Ok(())
}

fn open_root_exit_handle(
    pid: u32,
    attach_fault: AttachFault,
) -> Result<OwnedHandle, ResourceError> {
    #[cfg(not(test))]
    let _ = attach_fault;
    #[cfg(test)]
    if attach_fault == AttachFault::ExitHandleOpen {
        return Err(ResourceError::io(
            "open root process exit handle",
            io::Error::other("injected root exit handle open failure"),
        ));
    }
    // SAFETY: the caller retains an owned handle that keeps this PID bound to the same process
    // object until the synchronizable handle has been opened.
    OwnedHandle::new(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        },
        "open root process exit handle",
    )
}

fn forget_root_generation(state: &mut RunState, root_id: Uuid) {
    state.active.retain(|root| root.id != root_id);
    state.exited_roots.remove(&root_id);
}

fn detach_root_generation(state: &mut RunState, root_id: Uuid) {
    if state.exited_roots.remove(&root_id) {
        return;
    }
    if let Some(root) = state.active.iter_mut().find(|root| root.id == root_id) {
        root.signal = None;
    }
}

fn mark_active(_state: &mut RunState, _violation: u8) {
    // The run-wide job intentionally has no memory/process limits: its notifications cannot
    // identify the responsible root. Limits live on each nested root job instead.
}

#[derive(Debug)]
pub(crate) struct WindowsSupervisor {
    run: Arc<WindowsRunJob>,
    root_job: OwnedHandle,
    completion_port: OwnedHandle,
    root_id: Uuid,
    signal: Arc<RootSignal>,
    pid: Option<u32>,
    terminated: bool,
    attach_fault: AttachFault,
}

impl WindowsSupervisor {
    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
        self.pid = Some(self.run.attach_root(
            self.root_job.raw(),
            self.root_id,
            &self.signal,
            child,
            self.attach_fault,
        )?);
        Ok(())
    }

    pub(crate) fn classify(
        &mut self,
        termination: ProcessTermination,
    ) -> Result<ProcessTermination, ResourceError> {
        self.pid.ok_or(ResourceError::MissingProcessId)?;
        self.run
            .classify_root(self.root_id, &self.signal, termination)
            .map(|termination| self.classify_root_notification(termination))
    }

    fn classify_root_notification(&self, termination: ProcessTermination) -> ProcessTermination {
        let mut violations = self.signal.violations.load(Ordering::Acquire);
        while let Ok(Some((message, _))) =
            WindowsRunJob::next_notification_from(self.completion_port.raw(), 0)
        {
            violations |= match message {
                JOB_OBJECT_MSG_JOB_MEMORY_LIMIT => MEMORY_VIOLATION,
                JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT => PROCESS_VIOLATION,
                _ => 0,
            };
        }
        if violations & MEMORY_VIOLATION != 0 {
            ProcessTermination::OutOfMemory
        } else if violations & PROCESS_VIOLATION != 0 {
            ProcessTermination::ProcessLimit
        } else {
            termination
        }
    }

    pub(crate) fn terminate(&mut self) -> Result<bool, ResourceError> {
        if self.terminated {
            return self.tree_is_quiescent();
        }
        terminate_job(self.root_job.raw(), "terminate nested root process job")?;
        self.terminated = true;
        if self.pid.is_some() {
            self.run.unregister_root(self.root_id);
        }
        self.tree_is_quiescent()
    }

    pub(crate) fn refresh_tree_quiescence_after_root_reap(&self) -> Result<bool, ResourceError> {
        self.tree_is_quiescent()
    }

    fn tree_is_quiescent(&self) -> Result<bool, ResourceError> {
        active_process_count(self.root_job.raw()).map(|active| active == 0)
    }
}

impl Drop for WindowsSupervisor {
    fn drop(&mut self) {
        let _ = self.terminate();
        if self.pid.is_some() {
            self.run.unregister_root(self.root_id);
        }
    }
}

#[derive(Debug)]
pub(super) struct OwnedHandle(isize);

impl OwnedHandle {
    pub(super) fn new(handle: HANDLE, operation: &'static str) -> Result<Self, ResourceError> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(ResourceError::io(operation, io::Error::last_os_error()))
        } else {
            Ok(Self(handle as isize))
        }
    }

    pub(super) fn raw(&self) -> HANDLE {
        self.0 as HANDLE
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: every OwnedHandle contains one live owned Win32 handle.
        unsafe {
            CloseHandle(self.raw());
        }
    }
}

fn create_job() -> Result<OwnedHandle, ResourceError> {
    // SAFETY: null security/name pointers request an unnamed Job Object.
    OwnedHandle::new(
        unsafe { CreateJobObjectW(null(), null()) },
        "create process job",
    )
}

fn create_limited_root_job(limits: ProcessLimits) -> Result<OwnedHandle, ResourceError> {
    let job = create_job()?;
    let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_JOB_MEMORY
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    information.BasicLimitInformation.ActiveProcessLimit = limits.max_processes;
    information.JobMemoryLimit = usize::try_from(limits.max_memory_bytes)
        .map_err(|_| ResourceError::InvalidLimit("max_memory"))?;
    set_extended_limits(job.raw(), &information, "configure nested root process job")?;
    Ok(job)
}

fn configure_run_job(job: HANDLE) -> Result<(), ResourceError> {
    let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    set_extended_limits(job, &information, "configure run-wide process job")
}

fn set_extended_limits(
    job: HANDLE,
    information: &JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    operation: &'static str,
) -> Result<(), ResourceError> {
    // SAFETY: information has the exact layout and length required by this information class.
    if unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(information).cast::<c_void>(),
            u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                .expect("Windows Job Object structure size fits u32"),
        )
    } == 0
    {
        Err(ResourceError::io(operation, io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

fn active_process_count(job: HANDLE) -> Result<u32, ResourceError> {
    let mut information: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
    // SAFETY: information has the exact layout and length required by this information class.
    if unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicAccountingInformation,
            (&raw mut information).cast::<c_void>(),
            u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
                .expect("Windows Job Object structure size fits u32"),
            null_mut(),
        )
    } == 0
    {
        Err(ResourceError::io(
            "query active Job Object process count",
            io::Error::last_os_error(),
        ))
    } else {
        Ok(information.ActiveProcesses)
    }
}

fn create_completion_port() -> Result<OwnedHandle, ResourceError> {
    // SAFETY: INVALID_HANDLE_VALUE creates a standalone completion port.
    OwnedHandle::new(
        unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, null_mut(), 0, 1) },
        "create Job Object completion port",
    )
}

fn associate_completion_port(job: HANDLE, port: HANDLE) -> Result<(), ResourceError> {
    let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
        CompletionKey: dangling_mut::<c_void>(),
        CompletionPort: port,
    };
    // SAFETY: association references a live completion port for the lifetime of the job.
    if unsafe {
        SetInformationJobObject(
            job,
            JobObjectAssociateCompletionPortInformation,
            (&raw const association).cast::<c_void>(),
            u32::try_from(size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>())
                .expect("Windows completion port association size fits u32"),
        )
    } == 0
    {
        Err(ResourceError::io(
            "associate Job Object completion port",
            io::Error::last_os_error(),
        ))
    } else {
        Ok(())
    }
}

fn terminate_job(job: HANDLE, operation: &'static str) -> Result<(), ResourceError> {
    // SAFETY: job is a live handle owned by a backend or supervisor.
    if unsafe { TerminateJobObject(job, 1) } == 0 {
        Err(ResourceError::io(operation, io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use camino::{Utf8Path, Utf8PathBuf};
    use hoimin_core::{
        CommandArg, EffectFailure, EffectId, ProcessLimits, ProcessTermination, RawRunLimits,
        RunLimits, RunProcess,
    };
    use tokio::process::Command;
    use uuid::Uuid;
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    use super::{
        ActiveRoot, AttachFault, OwnedHandle, RootSignal, RunState, WindowsBackend,
        active_process_count, detach_root_generation, forget_root_generation, record_notification,
        root_process_signaled,
    };
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

    fn run_limits() -> RunLimits {
        RunLimits::try_from(&RawRunLimits::default()).unwrap()
    }

    fn fixture_limits() -> ProcessLimits {
        ProcessLimits {
            timeout: Duration::from_secs(30),
            max_output_bytes: 64,
            max_memory_bytes: 256 * 1024 * 1024,
            max_processes: 8,
        }
    }

    fn fixture_python_path() -> std::path::PathBuf {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let configuration = std::fs::read_to_string(workspace.join(".venv/pyvenv.cfg")).unwrap();
        let home = configuration
            .lines()
            .find_map(|line| line.strip_prefix("home = "))
            .expect("virtual environment records its base interpreter");
        let executable = std::path::Path::new(home).join("python.exe");
        assert!(
            executable.is_file(),
            "base fixture interpreter does not exist: {}",
            executable.display()
        );
        executable
    }

    fn fixture_python() -> CommandArg {
        arg(fixture_python_path())
    }

    fn sleeping_fixture(id: u64) -> RunProcess {
        RunProcess {
            id: EffectId(id),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![arg("ping.exe"), arg("-n"), arg("30"), arg("127.0.0.1")],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: fixture_limits(),
        }
    }

    fn abnormal_fixture(id: u64, ready: &Utf8Path, release: &Utf8Path) -> RunProcess {
        RunProcess {
            id: EffectId(id),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                fixture_python(),
                arg("-c"),
                arg(
                    "import ctypes,ctypes.wintypes as w,os,pathlib,sys,time; ready=pathlib.Path(sys.argv[1]); release=pathlib.Path(sys.argv[2]); pending=ready.with_suffix('.pending'); pending.write_text(str(os.getpid())); os.replace(pending,ready);\nwhile not release.exists(): time.sleep(0.005)\nkernel32=ctypes.WinDLL('kernel32',use_last_error=True); kernel32.GetCurrentProcess.argtypes=(); kernel32.GetCurrentProcess.restype=w.HANDLE; kernel32.TerminateProcess.argtypes=(w.HANDLE,w.UINT); kernel32.TerminateProcess.restype=w.BOOL; STATUS_ACCESS_VIOLATION=0xC0000005; kernel32.TerminateProcess(kernel32.GetCurrentProcess(),STATUS_ACCESS_VIOLATION)",
                ),
                arg(ready.as_std_path()),
                arg(release.as_std_path()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: fixture_limits(),
        }
    }

    struct HandlerCloseGuard(Arc<ProcessHandler>);

    impl Drop for HandlerCloseGuard {
        fn drop(&mut self) {
            let _ = self.0.close();
        }
    }

    async fn wait_for_ready_pid<F>(
        path: &Utf8Path,
        mut process: std::pin::Pin<&mut F>,
        deadline: tokio::time::Instant,
    ) -> Result<u32, String>
    where
        F: std::future::Future,
        F::Output: std::fmt::Debug,
    {
        loop {
            if let Some(pid) = std::fs::read_to_string(path)
                .ok()
                .and_then(|value| value.trim().parse::<u32>().ok())
                .filter(|pid| *pid != 0)
            {
                return Ok(pid);
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(format!(
                    "fixture did not atomically publish readiness before the test deadline at {path}"
                ));
            }
            tokio::select! {
                result = process.as_mut() => {
                    return Err(format!(
                        "fixture process completed before publishing readiness at {path}: {result:?}"
                    ));
                }
                () = tokio::time::sleep_until((now + Duration::from_millis(10)).min(deadline)) => {}
            }
        }
    }

    #[tokio::test]
    async fn fixture_readiness_uses_the_callers_absolute_deadline() {
        let temporary = tempfile::tempdir().unwrap();
        let path = Utf8Path::from_path(temporary.path())
            .unwrap()
            .join("never-ready");
        let deadline = tokio::time::Instant::now() + Duration::from_millis(25);
        let process = std::future::pending::<Result<(), &'static str>>();
        tokio::pin!(process);
        let started = Instant::now();

        let failure = tokio::time::timeout(
            Duration::from_millis(500),
            wait_for_ready_pid(&path, process.as_mut(), deadline),
        )
        .await
        .expect("readiness wait ignored both the caller deadline and the test harness bound")
        .expect_err("missing readiness must reach the caller's deadline");

        assert!(failure.contains("test deadline"), "{failure}");
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "readiness wait ignored the caller's deadline: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn fixture_readiness_reports_process_completion_before_publication() {
        let temporary = tempfile::tempdir().unwrap();
        let path = Utf8Path::from_path(temporary.path())
            .unwrap()
            .join("never-ready");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let process = std::future::ready::<Result<(), &'static str>>(Err("spawn failed"));
        tokio::pin!(process);
        let started = Instant::now();

        let failure = tokio::time::timeout(
            Duration::from_millis(500),
            wait_for_ready_pid(&path, process.as_mut(), deadline),
        )
        .await
        .expect("readiness wait ignored early process completion")
        .expect_err("early completion must be reported before readiness");

        assert!(failure.contains("spawn failed"), "{failure}");
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "early process completion was not observed immediately: {:?}",
            started.elapsed()
        );
    }

    async fn wait_for_job_process_count_until(
        backend: &WindowsBackend,
        expected: u32,
        deadline: tokio::time::Instant,
    ) -> Result<(), String> {
        loop {
            let active = active_process_count(backend.inner.job.raw()).unwrap();
            if active == expected {
                return Ok(());
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(format!(
                    "run Job Object has {active} assigned process(es) at the test deadline, expected {expected}"
                ));
            }
            tokio::time::sleep_until((now + Duration::from_millis(10)).min(deadline)).await;
        }
    }

    async fn wait_for_job_process_count<F>(
        backend: &WindowsBackend,
        expected: u32,
        mut process: std::pin::Pin<&mut F>,
        deadline: tokio::time::Instant,
    ) -> Result<(), String>
    where
        F: std::future::Future,
        F::Output: std::fmt::Debug,
    {
        tokio::select! {
            result = wait_for_job_process_count_until(backend, expected, deadline) => result,
            result = process.as_mut() => Err(format!(
                "fixture process completed before the run Job Object reached {expected} assigned process(es): {result:?}"
            )),
        }
    }

    #[tokio::test]
    async fn job_process_count_wait_uses_the_callers_absolute_deadline() {
        let backend = WindowsBackend::new(&run_limits()).unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(25);
        let process = std::future::pending::<Result<(), &'static str>>();
        tokio::pin!(process);
        let started = Instant::now();

        let failure = tokio::time::timeout(
            Duration::from_millis(500),
            wait_for_job_process_count(&backend, 1, process.as_mut(), deadline),
        )
        .await
        .expect("Job Object wait ignored both the caller deadline and the test harness bound")
        .expect_err("missing Job Object process must reach the caller's deadline");

        assert!(failure.contains("test deadline"), "{failure}");
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "Job Object wait ignored the caller's deadline: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn job_process_count_wait_reports_process_completion_before_assignment() {
        let backend = WindowsBackend::new(&run_limits()).unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let process = std::future::ready::<Result<(), &'static str>>(Err("attach failed"));
        tokio::pin!(process);
        let started = Instant::now();

        let failure = tokio::time::timeout(
            Duration::from_millis(500),
            wait_for_job_process_count(&backend, 1, process.as_mut(), deadline),
        )
        .await
        .expect("Job Object wait ignored early process completion")
        .expect_err("early completion must be reported before Job Object assignment");

        assert!(failure.contains("attach failed"), "{failure}");
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "early process completion was not observed immediately: {:?}",
            started.elapsed()
        );
    }

    fn job_process_ids(backend: &WindowsBackend) -> Vec<u32> {
        #[repr(C)]
        struct ProcessIds {
            assigned: u32,
            count: u32,
            values: [usize; 16],
        }
        let mut ids = ProcessIds {
            assigned: 0,
            count: 0,
            values: [0; 16],
        };
        // SAFETY: the output buffer is live and large enough for the fixed fixture limit.
        let ok = unsafe {
            windows_sys::Win32::System::JobObjects::QueryInformationJobObject(
                backend.inner.job.raw(),
                windows_sys::Win32::System::JobObjects::JobObjectBasicProcessIdList,
                (&raw mut ids).cast(),
                u32::try_from(std::mem::size_of::<ProcessIds>()).unwrap(),
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "query fixture Job Object process IDs");
        ids.values[..usize::try_from(ids.count).unwrap()]
            .iter()
            .map(|pid| u32::try_from(*pid).unwrap())
            .collect()
    }

    struct FixtureProcessHandle(OwnedHandle);

    impl FixtureProcessHandle {
        fn open(pid: u32) -> Self {
            // SAFETY: the PID was atomically published by a live fixture, and the owned handle is
            // validated below before use.
            let handle = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    pid,
                )
            };
            Self(OwnedHandle::new(handle, "open published fixture process").unwrap())
        }

        fn wait_result(&self) -> u32 {
            // SAFETY: the handle remains owned by this fixture wrapper for the duration of the wait.
            unsafe { WaitForSingleObject(self.0.raw(), 0) }
        }

        fn is_active(&self) -> bool {
            match self.wait_result() {
                WAIT_TIMEOUT => true,
                WAIT_OBJECT_0 => false,
                result => panic!("unexpected fixture process wait result: {result}"),
            }
        }

        async fn wait_until_exits(&self, deadline: tokio::time::Instant) -> bool {
            loop {
                match self.wait_result() {
                    WAIT_OBJECT_0 => return true,
                    WAIT_TIMEOUT if tokio::time::Instant::now() < deadline => {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    WAIT_TIMEOUT => return false,
                    result => panic!("unexpected fixture process wait result: {result}"),
                }
            }
        }
    }

    fn published_process_identities(path: &Utf8Path) -> Option<(u32, u32)> {
        let contents = fs::read_to_string(path).ok()?;
        let mut identities = contents.split_ascii_whitespace();
        let root = identities.next()?.parse().ok()?;
        let descendant = identities.next()?.parse().ok()?;
        identities.next().is_none().then_some((root, descendant))
    }

    #[tokio::test]
    async fn exited_root_is_observed_before_assigned_descendant_cleanup() {
        let temporary = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
        let identities = output_dir.join("root-and-descendant.txt");
        let release = output_dir.join("release-root");
        let backend = WindowsBackend::new(&run_limits()).unwrap();
        let code = "import os,pathlib,subprocess,sys,time; identities=pathlib.Path(sys.argv[1]); release=pathlib.Path(sys.argv[2]); child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL); pending=identities.with_suffix('.pending'); pending.write_text(f'{os.getpid()} {child.pid}',encoding='utf-8'); os.replace(pending,identities);\nwhile not release.exists(): time.sleep(0.005)";
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(6);

        tokio::time::timeout_at(deadline, async {
            let mut command = Command::new(fixture_python_path());
            command
                .arg("-c")
                .arg(code)
                .arg(identities.as_std_path())
                .arg(release.as_std_path())
                .current_dir(std::env::current_dir().unwrap())
                .kill_on_drop(true)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut supervisor = backend.prepare(&mut command, fixture_limits()).unwrap();
            let mut child = command.spawn().unwrap();
            supervisor.attach(&child).unwrap();

            let (root_pid, descendant_pid) = loop {
                if let Some(identities) = published_process_identities(&identities) {
                    break identities;
                }
                let now = tokio::time::Instant::now();
                assert!(
                    now < deadline,
                    "fixture did not publish root and descendant identities before the test deadline"
                );
                tokio::time::sleep_until((now + Duration::from_millis(5)).min(deadline)).await;
            };
            assert_eq!(child.id(), Some(root_pid));
            let root = FixtureProcessHandle::open(root_pid);
            let descendant = FixtureProcessHandle::open(descendant_pid);
            fs::write(&release, b"exit").unwrap();

            let status = tokio::time::timeout_at(deadline, child.wait())
                .await
                .expect("runtime root did not exit before the test deadline")
                .unwrap();
            assert!(
                !root.is_active(),
                "runtime root process handle remained active after child.wait completed"
            );
            assert!(
                descendant.is_active(),
                "assigned descendant exited before root termination was classified"
            );

            let termination = supervisor
                .classify(crate::process::exit_termination(status))
                .unwrap();
            assert_eq!(termination, ProcessTermination::Exit(0));
            assert!(
                descendant.is_active(),
                "root classification terminated or waited for the assigned descendant"
            );
            assert!(
                !supervisor
                    .refresh_tree_quiescence_after_root_reap()
                    .unwrap(),
                "an active assigned descendant was reported as quiescent"
            );

            supervisor.terminate(false).unwrap();
            assert!(
                descendant.wait_until_exits(deadline).await,
                "assigned descendant remained active after nested Job Object termination"
            );
            wait_for_job_process_count_until(&backend, 0, deadline)
                .await
                .unwrap();
            backend.close().unwrap();
            assert_eq!(active_process_count(backend.inner.job.raw()).unwrap(), 0);
        })
        .await
        .expect("root-before-descendant handling and Job Object cleanup exceeded six seconds");
        assert!(started.elapsed() < Duration::from_secs(6));
    }

    #[tokio::test]
    async fn abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds()
    {
        let temporary = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
        let abnormal_ready = output_dir.join("abnormal-root.ready");
        let abnormal_release = output_dir.join("release-abnormal-root");
        let backend = WindowsBackend::new(&run_limits()).unwrap();
        let handler = Arc::new(ProcessHandler::new(
            ResourceBackend::Windows(backend.clone()),
            output_dir.to_owned(),
        ));
        let _cleanup = HandlerCloseGuard(Arc::clone(&handler));
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
        let phase = std::cell::Cell::new("starting sibling root");

        let outcome = tokio::time::timeout_at(deadline, async {
            let sibling = handler.handle(sleeping_fixture(260));
            tokio::pin!(sibling);
            wait_for_job_process_count(&backend, 1, sibling.as_mut(), deadline).await?;

            phase.set("waiting for abnormal-root readiness");
            let handle_started = Instant::now();
            let abnormal =
                handler.handle(abnormal_fixture(261, &abnormal_ready, &abnormal_release));
            tokio::pin!(abnormal);
            let abnormal_pid =
                wait_for_ready_pid(&abnormal_ready, abnormal.as_mut(), deadline).await?;
            phase.set("confirming abnormal-root Job Object assignment");
            wait_for_job_process_count(&backend, 2, abnormal.as_mut(), deadline).await?;
            assert!(
                job_process_ids(&backend).contains(&abnormal_pid),
                "published runtime PID is not the root assigned to the production Job Object"
            );
            std::fs::write(&abnormal_release, b"abort").unwrap();
            phase.set("classifying abnormal-root termination");
            let abnormal = abnormal
                .await
                .map_err(|error| format!("abnormal fixture failed after release: {error:?}"))?;
            assert!(handle_started.elapsed() < Duration::from_secs(6));
            assert_eq!(
                abnormal.termination,
                ProcessTermination::Exit(-1_073_741_819)
            );
            phase.set("confirming sibling remains after abnormal-root exit");
            wait_for_job_process_count(&backend, 1, sibling.as_mut(), deadline).await?;

            phase.set("closing the run Job Object");
            let close_started = Instant::now();
            handler
                .close()
                .map_err(|error| format!("close the run Job Object: {error}"))?;
            assert!(close_started.elapsed() < Duration::from_secs(6));
            phase.set("reaping sibling after close");
            sibling.await.map_err(|error| {
                format!("sibling handle did not complete after close: {error:?}")
            })?;
            phase.set("confirming the run Job Object is empty");
            wait_for_job_process_count_until(&backend, 0, deadline).await?;
            Ok::<(), String>(())
        })
        .await;
        let failure = match outcome {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(_) => Some("the shared six-second deadline expired".to_owned()),
        };
        if let Some(failure) = failure {
            let readiness = std::fs::read_to_string(&abnormal_ready);
            let active = active_process_count(backend.inner.job.raw());
            let cleanup = handler.close();
            panic!(
                "abnormal Job Object fixture failed while {}; failure={failure}; readiness={readiness:?}; active_processes={active:?}; emergency_close={cleanup:?}",
                phase.get(),
            );
        }
        assert!(started.elapsed() < Duration::from_secs(6));
    }

    #[tokio::test]
    async fn attach_failure_cleanup_retains_signaled_assigned_generation() {
        for (sequence, fault) in [
            AttachFault::Assign,
            AttachFault::NestedAssign,
            AttachFault::Resume,
        ]
        .into_iter()
        .enumerate()
        {
            let temporary = tempfile::tempdir().unwrap();
            let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
            let marker = output_dir.join(format!("fault-{sequence}.marker"));
            let backend = WindowsBackend::with_test_fault(&run_limits(), fault).unwrap();
            let handler = ProcessHandler::new(
                ResourceBackend::Windows(backend.clone()),
                output_dir.to_owned(),
            );
            let request = RunProcess {
                id: EffectId(200 + sequence as u64),
                worker: None,
                run_id: None,
                mutant_id: None,
                argv: vec![
                    python(),
                    arg("-c"),
                    arg(
                        "import pathlib,subprocess,sys,time; pathlib.Path(sys.argv[1]).write_text('ran'); subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); time.sleep(30)",
                    ),
                    arg(marker.as_std_path()),
                ],
                cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
                limits: ProcessLimits {
                    timeout: Duration::from_secs(10),
                    max_output_bytes: 64,
                    max_memory_bytes: 256 * 1024 * 1024,
                    max_processes: 8,
                },
            };
            let started = Instant::now();

            let error = tokio::time::timeout(Duration::from_secs(2), handler.handle(request))
                .await
                .expect("attach failure cleanup is bounded")
                .expect_err("fault is returned");

            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(!marker.exists(), "suspended user code must not execute");
            assert!(matches!(
                error.failure,
                EffectFailure::Io { ref code, .. } if code == "process.resource.attach"
            ));

            let state = backend
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if fault == AttachFault::Assign {
                assert!(state.active.is_empty());
            } else {
                assert_eq!(state.active.len(), 1);
                assert_ne!(state.active[0].id, Uuid::nil());
                assert_ne!(state.active[0].pid, 0);
                assert!(state.active[0].signal.is_none());
                assert!(state.active[0].process.is_some());
                let root_id = state.active[0].id;
                assert!(
                    root_process_signaled(&state, root_id).unwrap(),
                    "retained generation process handle must be signaled"
                );
            }
            assert!(state.exited_roots.is_empty());
        }
    }

    #[tokio::test]
    async fn exit_handle_reopen_failure_preserves_detached_generation_identity() {
        let temporary = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
        let backend =
            WindowsBackend::with_test_fault(&run_limits(), AttachFault::ExitHandleOpen).unwrap();
        let handler = ProcessHandler::new(
            ResourceBackend::Windows(backend.clone()),
            output_dir.to_owned(),
        );

        let error = tokio::time::timeout(
            Duration::from_secs(2),
            handler.handle(sleeping_fixture(270)),
        )
        .await
        .expect("exit-handle reopen failure cleanup is bounded")
        .expect_err("injected exit-handle reopen failure is returned");

        assert!(matches!(
            error.failure,
            EffectFailure::Io {
                ref code,
                ref message,
                ..
            } if code == "process.resource.attach"
                && message.contains("injected root exit handle open failure")
                && !message.contains("spawned-child cleanup failed")
        ));
        let state = backend
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(state.active.len(), 1);
        assert_ne!(state.active[0].id, Uuid::nil());
        assert_ne!(state.active[0].pid, 0);
        assert!(state.active[0].signal.is_none());
        assert!(state.active[0].process.is_some());
        assert!(state.exited_roots.is_empty());
    }

    #[tokio::test]
    async fn timeout_cleanup_retains_signaled_detached_generation() {
        let temporary = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
        let backend = WindowsBackend::new(&run_limits()).unwrap();
        let handler = ProcessHandler::new(
            ResourceBackend::Windows(backend.clone()),
            output_dir.to_owned(),
        );
        let request = RunProcess {
            id: EffectId(250),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![python(), arg("-c"), arg("import time; time.sleep(30)")],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: ProcessLimits {
                timeout: Duration::from_millis(100),
                max_output_bytes: 64,
                max_memory_bytes: 256 * 1024 * 1024,
                max_processes: 8,
            },
        };

        let event = handler.handle(request).await.unwrap();
        assert_eq!(event.termination, hoimin_core::ProcessTermination::Timeout);

        let state = backend
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(state.active.len(), 1);
        assert_ne!(state.active[0].id, Uuid::nil());
        assert_ne!(state.active[0].pid, 0);
        assert!(state.active[0].signal.is_none());
        assert!(state.active[0].process.is_some());
        let root_id = state.active[0].id;
        assert!(
            root_process_signaled(&state, root_id).unwrap(),
            "retained generation process handle must be signaled"
        );
        assert!(state.exited_roots.is_empty());
    }

    #[test]
    fn signaled_root_without_exit_notification_classifies_and_retains_generation() {
        let backend = WindowsBackend::new(&run_limits()).unwrap();
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "exit 7"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let process = FixtureProcessHandle::open(pid);
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(7));
        assert_eq!(
            process.wait_result(),
            WAIT_OBJECT_0,
            "exited fixture process handle must be signaled before classification"
        );

        let root_id = Uuid::from_u128(1_201);
        let signal = Arc::new(RootSignal::default());
        backend
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active
            .push(ActiveRoot {
                id: root_id,
                pid,
                signal: Some(Arc::downgrade(&signal)),
                process: Some(process.0),
            });

        let termination = ProcessTermination::Exit(7);
        assert_eq!(
            backend
                .inner
                .classify_root(root_id, &signal, termination)
                .unwrap(),
            termination
        );

        let state = backend
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(state.active.len(), 1);
        assert_eq!(state.active[0].id, root_id);
        assert_eq!(state.active[0].pid, pid);
        assert!(state.active[0].signal.is_none());
        assert!(state.active[0].process.is_some());
        assert!(state.exited_roots.is_empty());
    }

    #[test]
    fn queued_exit_notification_wins_over_signaled_process_fallback() {
        let backend = WindowsBackend::new(&run_limits()).unwrap();
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "exit 8"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let process = FixtureProcessHandle::open(pid);
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(8));
        assert_eq!(
            process.wait_result(),
            WAIT_OBJECT_0,
            "exited fixture process handle must be signaled before classification"
        );

        let root_id = Uuid::from_u128(1_202);
        let signal = Arc::new(RootSignal::default());
        backend
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active
            .push(ActiveRoot {
                id: root_id,
                pid,
                signal: Some(Arc::downgrade(&signal)),
                process: Some(process.0),
            });
        // SAFETY: the completion port is live, and this test posts the same PID-shaped value that
        // the Job Object completion protocol supplies through the overlapped pointer.
        let posted = unsafe {
            PostQueuedCompletionStatus(
                backend.inner.completion_port.raw(),
                windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_EXIT_PROCESS,
                0,
                pid as usize as *mut _,
            )
        };
        assert_ne!(posted, 0, "post queued root exit notification");

        let termination = ProcessTermination::Exit(8);
        assert_eq!(
            backend
                .inner
                .classify_root(root_id, &signal, termination)
                .unwrap(),
            termination
        );

        let state = backend
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(state.active.is_empty());
        assert!(state.exited_roots.is_empty());
    }

    #[test]
    fn running_root_process_handle_is_not_classified_as_signaled() {
        let mut child = std::process::Command::new("ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let process = FixtureProcessHandle::open(pid);
        assert_eq!(
            process.wait_result(),
            WAIT_TIMEOUT,
            "running fixture process handle must not be signaled"
        );

        let root_id = Uuid::from_u128(1_203);
        let signal = Arc::new(RootSignal::default());
        let state = RunState {
            active: vec![ActiveRoot {
                id: root_id,
                pid,
                signal: Some(Arc::downgrade(&signal)),
                process: Some(process.0),
            }],
            ..RunState::default()
        };

        assert!(!root_process_signaled(&state, root_id).unwrap());

        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn close_failure_keeps_run_closed_and_retries_termination() {
        let backend = WindowsBackend::with_test_close_failure(&run_limits()).unwrap();

        let first = backend.close().expect_err("first termination is injected");
        assert!(
            first
                .to_string()
                .contains("injected run termination failure")
        );
        {
            let state = backend
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert!(state.closed, "failed close still rejects future spawn");
            assert!(!state.terminated, "OS termination has not succeeded yet");
        }

        backend.close().expect("second close retries termination");
        let state = backend
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(state.closed);
        assert!(state.terminated);
    }

    #[test]
    fn run_wide_violation_does_not_mark_any_active_root() {
        let exited = std::sync::Arc::new(RootSignal::default());
        let active = std::sync::Arc::new(RootSignal::default());
        let exited_id = Uuid::from_u128(301);
        let mut state = RunState {
            active: vec![
                ActiveRoot {
                    id: exited_id,
                    pid: 301,
                    signal: Some(std::sync::Arc::downgrade(&exited)),
                    process: None,
                },
                ActiveRoot {
                    id: Uuid::from_u128(302),
                    pid: 302,
                    signal: Some(std::sync::Arc::downgrade(&active)),
                    process: None,
                },
            ],
            ..RunState::default()
        };

        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_EXIT_PROCESS,
            301,
            false,
        );
        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
            0,
            false,
        );

        assert_eq!(exited.violations.load(Ordering::Acquire), 0);
        assert_eq!(active.violations.load(Ordering::Acquire), 0);
        assert!(state.exited_roots.contains(&exited_id));
    }

    #[test]
    fn abnormal_exit_marks_only_its_root_while_other_roots_remain_active() {
        let crashed = std::sync::Arc::new(RootSignal::default());
        let running = std::sync::Arc::new(RootSignal::default());
        let crashed_id = Uuid::from_u128(401);
        let mut state = RunState {
            active: vec![
                ActiveRoot {
                    id: crashed_id,
                    pid: 401,
                    signal: Some(std::sync::Arc::downgrade(&crashed)),
                    process: None,
                },
                ActiveRoot {
                    id: Uuid::from_u128(402),
                    pid: 402,
                    signal: Some(std::sync::Arc::downgrade(&running)),
                    process: None,
                },
            ],
            ..RunState::default()
        };

        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS,
            401,
            false,
        );

        assert_eq!(
            state.active.iter().map(|root| root.pid).collect::<Vec<_>>(),
            vec![402]
        );
        assert_eq!(state.exited_roots.len(), 1);
        assert!(state.exited_roots.contains(&crashed_id));
    }

    #[test]
    fn recycled_pid_exit_generations_are_recorded_independently() {
        let first = std::sync::Arc::new(RootSignal::default());
        let second = std::sync::Arc::new(RootSignal::default());
        let first_id = Uuid::from_u128(501);
        let second_id = Uuid::from_u128(502);
        let mut state = RunState {
            active: vec![ActiveRoot {
                id: first_id,
                pid: 501,
                signal: Some(std::sync::Arc::downgrade(&first)),
                process: None,
            }],
            ..RunState::default()
        };

        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_EXIT_PROCESS,
            501,
            false,
        );
        state.active.push(ActiveRoot {
            id: second_id,
            pid: 501,
            signal: Some(std::sync::Arc::downgrade(&second)),
            process: None,
        });
        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_EXIT_PROCESS,
            501,
            false,
        );

        assert!(state.active.is_empty());
        assert_eq!(
            state.exited_roots.len(),
            2,
            "each PID generation needs independent completion state"
        );
        assert!(state.exited_roots.contains(&first_id));
        assert!(state.exited_roots.contains(&second_id));
    }

    #[test]
    fn old_generation_cleanup_preserves_a_new_root_with_the_same_pid() {
        let old = std::sync::Arc::new(RootSignal::default());
        let new = std::sync::Arc::new(RootSignal::default());
        let old_id = Uuid::from_u128(601);
        let new_id = Uuid::from_u128(602);
        let mut state = RunState {
            active: vec![ActiveRoot {
                id: old_id,
                pid: 601,
                signal: Some(std::sync::Arc::downgrade(&old)),
                process: None,
            }],
            ..RunState::default()
        };
        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_EXIT_PROCESS,
            601,
            false,
        );
        state.active.push(ActiveRoot {
            id: new_id,
            pid: 601,
            signal: Some(std::sync::Arc::downgrade(&new)),
            process: None,
        });

        forget_root_generation(&mut state, old_id);

        assert_eq!(
            state.active.iter().map(|root| root.id).collect::<Vec<_>>(),
            vec![new_id]
        );
        assert!(state.exited_roots.is_empty());
    }

    #[test]
    fn unrelated_exit_preserves_registration_order_for_a_recycled_pid() {
        let other = std::sync::Arc::new(RootSignal::default());
        let old = std::sync::Arc::new(RootSignal::default());
        let new = std::sync::Arc::new(RootSignal::default());
        let other_id = Uuid::from_u128(701);
        let old_id = Uuid::from_u128(702);
        let new_id = Uuid::from_u128(703);
        let mut state = RunState {
            active: vec![
                ActiveRoot {
                    id: other_id,
                    pid: 700,
                    signal: Some(std::sync::Arc::downgrade(&other)),
                    process: None,
                },
                ActiveRoot {
                    id: old_id,
                    pid: 701,
                    signal: Some(std::sync::Arc::downgrade(&old)),
                    process: None,
                },
                ActiveRoot {
                    id: new_id,
                    pid: 701,
                    signal: Some(std::sync::Arc::downgrade(&new)),
                    process: None,
                },
            ],
            ..RunState::default()
        };

        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_EXIT_PROCESS,
            700,
            false,
        );
        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_EXIT_PROCESS,
            701,
            false,
        );

        assert!(state.exited_roots.contains(&other_id));
        assert!(state.exited_roots.contains(&old_id));
        assert!(!state.exited_roots.contains(&new_id));
        assert_eq!(
            state.active.iter().map(|root| root.id).collect::<Vec<_>>(),
            vec![new_id]
        );
    }

    #[test]
    fn detached_generation_consumes_its_delayed_exit_before_a_reused_pid() {
        let old = std::sync::Arc::new(RootSignal::default());
        let new = std::sync::Arc::new(RootSignal::default());
        let old_id = Uuid::from_u128(801);
        let new_id = Uuid::from_u128(802);
        let mut state = RunState {
            active: vec![ActiveRoot {
                id: old_id,
                pid: 801,
                signal: Some(std::sync::Arc::downgrade(&old)),
                process: None,
            }],
            ..RunState::default()
        };

        detach_root_generation(&mut state, old_id);
        state.active.push(ActiveRoot {
            id: new_id,
            pid: 801,
            signal: Some(std::sync::Arc::downgrade(&new)),
            process: None,
        });
        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_EXIT_PROCESS,
            801,
            false,
        );

        assert!(!state.exited_roots.contains(&old_id));
        assert!(!state.exited_roots.contains(&new_id));
        assert_eq!(state.active.len(), 1);
        assert_eq!(state.active[0].id, new_id);
    }

    #[test]
    fn stale_active_process_zero_does_not_clear_a_new_root() {
        let root = std::sync::Arc::new(RootSignal::default());
        let root_id = Uuid::from_u128(901);
        let mut state = RunState {
            active: vec![ActiveRoot {
                id: root_id,
                pid: 901,
                signal: Some(std::sync::Arc::downgrade(&root)),
                process: None,
            }],
            ..RunState::default()
        };

        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO,
            0,
            false,
        );

        assert_eq!(state.active.len(), 1);
        assert_eq!(state.active[0].id, root_id);
        assert!(state.exited_roots.is_empty());
    }

    #[test]
    fn confirmed_active_process_zero_completes_all_registered_roots() {
        let first = std::sync::Arc::new(RootSignal::default());
        let second = std::sync::Arc::new(RootSignal::default());
        let first_id = Uuid::from_u128(1001);
        let second_id = Uuid::from_u128(1002);
        let mut state = RunState {
            active: vec![
                ActiveRoot {
                    id: first_id,
                    pid: 1001,
                    signal: Some(std::sync::Arc::downgrade(&first)),
                    process: None,
                },
                ActiveRoot {
                    id: second_id,
                    pid: 1002,
                    signal: Some(std::sync::Arc::downgrade(&second)),
                    process: None,
                },
            ],
            ..RunState::default()
        };

        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO,
            0,
            true,
        );

        assert!(state.active.is_empty());
        assert_eq!(state.exited_roots.len(), 2);
        assert!(state.exited_roots.contains(&first_id));
        assert!(state.exited_roots.contains(&second_id));
    }

    #[test]
    fn confirmed_active_process_zero_discards_detached_generations() {
        let detached = std::sync::Arc::new(RootSignal::default());
        let live = std::sync::Arc::new(RootSignal::default());
        let detached_id = Uuid::from_u128(1101);
        let live_id = Uuid::from_u128(1102);
        let mut state = RunState {
            active: vec![
                ActiveRoot {
                    id: detached_id,
                    pid: 1101,
                    signal: Some(std::sync::Arc::downgrade(&detached)),
                    process: None,
                },
                ActiveRoot {
                    id: live_id,
                    pid: 1102,
                    signal: Some(std::sync::Arc::downgrade(&live)),
                    process: None,
                },
            ],
            ..RunState::default()
        };
        detach_root_generation(&mut state, detached_id);

        record_notification(
            &mut state,
            windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO,
            0,
            true,
        );

        assert!(state.active.is_empty());
        assert_eq!(state.exited_roots.len(), 1);
        assert!(!state.exited_roots.contains(&detached_id));
        assert!(state.exited_roots.contains(&live_id));
    }
}
