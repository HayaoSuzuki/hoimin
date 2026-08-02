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
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT};
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
}

impl WindowsBackend {
    /// # Errors
    ///
    /// Returns an error when configured limits cannot be represented by the Windows Job Object API
    /// or its run-wide job cannot be created.
    pub fn new(limits: &RunLimits) -> Result<Self, ResourceError> {
        let memory = usize::try_from(limits.max_memory.get())
            .map_err(|_| ResourceError::InvalidLimit("max_memory"))?;
        let processes = u32::try_from(limits.max_processes.get())
            .map_err(|_| ResourceError::InvalidLimit("max_processes"))?;
        Ok(Self {
            inner: Arc::new(WindowsRunJob::new(memory, processes)?),
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
        _limits: ProcessLimits,
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
        Ok(ProcessSupervisor::Windows(WindowsSupervisor {
            run: Arc::clone(&self.inner),
            root_job: create_kill_on_close_job()?,
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
    _process: Option<OwnedHandle>,
}

#[derive(Debug, Default)]
struct RootSignal {
    violations: AtomicU8,
}

impl WindowsRunJob {
    fn new(memory: usize, processes: u32) -> Result<Self, ResourceError> {
        let job = create_job()?;
        configure_run_job(job.raw(), memory, processes)?;
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
            register_root(&mut state, root_id, pid, None, child);
            return Err(ResourceError::io(
                "assign process to nested root job",
                io::Error::other("injected nested assignment failure"),
            ));
        }
        if let Err(error) = child.assign(root_job, "assign process to nested root job") {
            register_root(&mut state, root_id, pid, None, child);
            return Err(error);
        }
        #[cfg(test)]
        if attach_fault == AttachFault::Resume {
            register_root(&mut state, root_id, pid, None, child);
            return Err(ResourceError::io(
                "resume suspended primary thread",
                io::Error::other("injected resume failure"),
            ));
        }
        if let Err(error) = child.resume() {
            register_root(&mut state, root_id, pid, None, child);
            return Err(error);
        }
        register_root(&mut state, root_id, pid, Some(signal), child);
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
        self.drain_until_root_exit(&mut state, root_id)?;
        forget_root_generation(&mut state, root_id);
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
    ) -> Result<(), ResourceError> {
        if root_notification_consumed(state, root_id) {
            return Ok(());
        }
        let deadline = Instant::now() + NOTIFICATION_BARRIER_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ResourceError::io(
                    "wait for Job Object exit notification",
                    io::Error::new(io::ErrorKind::TimedOut, "root exit notification timed out"),
                ));
            }
            let timeout_ms = u32::try_from(remaining.as_millis().min(u128::from(u32::MAX)))
                .expect("bounded timeout fits u32");
            match self.next_notification(timeout_ms.max(1))? {
                Some((message, pid)) => {
                    self.record_notification(state, message, pid)?;
                    if root_notification_consumed(state, root_id) {
                        return Ok(());
                    }
                }
                None => {
                    return Err(ResourceError::io(
                        "wait for Job Object exit notification",
                        io::Error::new(io::ErrorKind::TimedOut, "root exit notification timed out"),
                    ));
                }
            }
        }
    }

    fn next_notification(&self, timeout_ms: u32) -> Result<Option<(u32, u32)>, ResourceError> {
        let mut message = 0_u32;
        let mut key = 0_usize;
        let mut overlapped = null_mut();
        // SAFETY: all pointers reference live local storage and the completion port is owned.
        let ok = unsafe {
            GetQueuedCompletionStatus(
                self.completion_port.raw(),
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
        JOB_OBJECT_MSG_EXIT_PROCESS | JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS
            if let Some(index) = state.active.iter().position(|root| root.pid == pid) =>
        {
            let root = state.active.remove(index);
            if root.signal.is_some() {
                state.exited_roots.insert(root.id);
            }
        }
        _ => {}
    }
}

fn root_notification_consumed(state: &RunState, root_id: Uuid) -> bool {
    state.exited_roots.contains(&root_id) || state.active.iter().all(|root| root.id != root_id)
}

fn register_root(
    state: &mut RunState,
    root_id: Uuid,
    pid: u32,
    signal: Option<&Arc<RootSignal>>,
    child: super::suspended::SuspendedChild,
) {
    state.active.push(ActiveRoot {
        id: root_id,
        pid,
        signal: signal.map(Arc::downgrade),
        _process: Some(child.into_process_handle()),
    });
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

fn mark_active(state: &mut RunState, violation: u8) {
    for root in &state.active {
        if let Some(signal) = root.signal.as_ref().and_then(Weak::upgrade) {
            // Aggregate Job notifications do not identify a culprit. Mark only roots active at
            // receipt as participants in a run-wide safety violation; future roots are unaffected.
            signal.violations.fetch_or(violation, Ordering::AcqRel);
        }
    }
}

#[derive(Debug)]
pub(crate) struct WindowsSupervisor {
    run: Arc<WindowsRunJob>,
    root_job: OwnedHandle,
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
    }

    pub(crate) fn terminate(&mut self) -> Result<(), ResourceError> {
        if self.terminated {
            return Ok(());
        }
        terminate_job(self.root_job.raw(), "terminate nested root process job")?;
        self.terminated = true;
        if self.pid.is_some() {
            self.run.unregister_root(self.root_id);
        }
        Ok(())
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

fn create_kill_on_close_job() -> Result<OwnedHandle, ResourceError> {
    let job = create_job()?;
    let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    set_extended_limits(job.raw(), &information, "configure nested root process job")?;
    Ok(job)
}

fn configure_run_job(job: HANDLE, memory: usize, processes: u32) -> Result<(), ResourceError> {
    let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_JOB_MEMORY
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    information.BasicLimitInformation.ActiveProcessLimit = processes;
    information.JobMemoryLimit = memory;
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
    use std::os::windows::ffi::OsStrExt;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use camino::{Utf8Path, Utf8PathBuf};
    use hoimin_core::{
        CommandArg, EffectFailure, EffectId, ProcessLimits, RawRunLimits, RunLimits, RunProcess,
    };
    use uuid::Uuid;

    use super::{
        ActiveRoot, AttachFault, MEMORY_VIOLATION, RootSignal, RunState, WindowsBackend,
        detach_root_generation, forget_root_generation, record_notification,
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

    #[tokio::test]
    async fn attach_failures_kill_suspended_root_and_retain_assigned_identity_until_exit() {
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

            let mut state = backend
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if fault == AttachFault::Assign {
                assert!(state.active.is_empty());
            } else {
                assert_eq!(state.active.len(), 1);
                assert!(state.active[0].signal.is_none());
                assert!(state.active[0]._process.is_some());
                let root_id = state.active[0].id;
                backend
                    .inner
                    .drain_until_root_exit(&mut state, root_id)
                    .unwrap();
                assert!(state.active.is_empty());
                assert!(state.exited_roots.is_empty());
            }
        }
    }

    #[tokio::test]
    async fn timed_out_root_retains_identity_until_its_exit_notification() {
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

        let mut state = backend
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(state.active.len(), 1);
        assert!(state.active[0].signal.is_none());
        assert!(state.active[0]._process.is_some());
        let root_id = state.active[0].id;
        backend
            .inner
            .drain_until_root_exit(&mut state, root_id)
            .unwrap();
        assert!(state.active.is_empty());
        assert!(state.exited_roots.is_empty());
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
    fn exited_root_is_not_marked_by_a_later_aggregate_violation() {
        let exited = std::sync::Arc::new(RootSignal::default());
        let active = std::sync::Arc::new(RootSignal::default());
        let exited_id = Uuid::from_u128(301);
        let mut state = RunState {
            active: vec![
                ActiveRoot {
                    id: exited_id,
                    pid: 301,
                    signal: Some(std::sync::Arc::downgrade(&exited)),
                    _process: None,
                },
                ActiveRoot {
                    id: Uuid::from_u128(302),
                    pid: 302,
                    signal: Some(std::sync::Arc::downgrade(&active)),
                    _process: None,
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
        assert_eq!(active.violations.load(Ordering::Acquire), MEMORY_VIOLATION);
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
                    _process: None,
                },
                ActiveRoot {
                    id: Uuid::from_u128(402),
                    pid: 402,
                    signal: Some(std::sync::Arc::downgrade(&running)),
                    _process: None,
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
                _process: None,
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
            _process: None,
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
                _process: None,
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
            _process: None,
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
                    _process: None,
                },
                ActiveRoot {
                    id: old_id,
                    pid: 701,
                    signal: Some(std::sync::Arc::downgrade(&old)),
                    _process: None,
                },
                ActiveRoot {
                    id: new_id,
                    pid: 701,
                    signal: Some(std::sync::Arc::downgrade(&new)),
                    _process: None,
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
                _process: None,
            }],
            ..RunState::default()
        };

        detach_root_generation(&mut state, old_id);
        state.active.push(ActiveRoot {
            id: new_id,
            pid: 801,
            signal: Some(std::sync::Arc::downgrade(&new)),
            _process: None,
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
                _process: None,
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
                    _process: None,
                },
                ActiveRoot {
                    id: second_id,
                    pid: 1002,
                    signal: Some(std::sync::Arc::downgrade(&second)),
                    _process: None,
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
                    _process: None,
                },
                ActiveRoot {
                    id: live_id,
                    pid: 1102,
                    signal: Some(std::sync::Arc::downgrade(&live)),
                    _process: None,
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
