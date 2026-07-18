use std::collections::HashSet;
use std::ffi::c_void;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::ptr::{dangling_mut, null, null_mut};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use hoimin_core::{ProcessLimits, ProcessTermination, ResourceMode, RunLimits};
use tokio::process::{Child, Command};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_ASSOCIATE_COMPLETION_PORT, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectAssociateCompletionPortInformation, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::SystemServices::{
    JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT, JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO,
    JOB_OBJECT_MSG_EXIT_PROCESS, JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_QUOTA, PROCESS_TERMINATE, ResumeThread, THREAD_SUSPEND_RESUME,
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
    Resume,
}

impl WindowsBackend {
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

    pub fn mode(&self) -> ResourceMode {
        ResourceMode::Hard
    }

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
                .unwrap_or_else(|error| error.into_inner());
            self.inner.drain_pending(&mut state)?;
            if state.closed {
                return Err(ResourceError::RunClosed);
            }
        }
        command.as_std_mut().creation_flags(CREATE_SUSPENDED);
        Ok(ProcessSupervisor::Windows(WindowsSupervisor {
            run: Arc::clone(&self.inner),
            root_job: create_kill_on_close_job()?,
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
    exited_roots: HashSet<u32>,
}

#[derive(Debug)]
struct ActiveRoot {
    pid: u32,
    signal: Weak<RootSignal>,
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
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
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
        signal: &Arc<RootSignal>,
        child: &Child,
        _attach_fault: AttachFault,
    ) -> Result<u32, ResourceError> {
        let pid = child.id().ok_or(ResourceError::MissingProcessId)?;
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        self.drain_pending(&mut state)?;
        if state.closed {
            return Err(ResourceError::RunClosed);
        }
        #[cfg(test)]
        if _attach_fault == AttachFault::Assign {
            return Err(ResourceError::io(
                "assign process to run-wide job",
                io::Error::other("injected assignment failure"),
            ));
        }
        let process = open_process(pid)?;
        assign_process(
            self.job.raw(),
            process.raw(),
            "assign process to run-wide job",
        )?;
        assign_process(root_job, process.raw(), "assign process to nested root job")?;
        state.active.push(ActiveRoot {
            pid,
            signal: Arc::downgrade(signal),
        });
        #[cfg(test)]
        if _attach_fault == AttachFault::Resume {
            state.active.retain(|root| root.pid != pid);
            return Err(ResourceError::io(
                "resume suspended primary thread",
                io::Error::other("injected resume failure"),
            ));
        }
        if let Err(error) = resume_primary_thread(pid) {
            state.active.retain(|root| root.pid != pid);
            return Err(error);
        }
        Ok(pid)
    }

    fn classify_root(
        &self,
        pid: u32,
        signal: &RootSignal,
        termination: ProcessTermination,
    ) -> Result<ProcessTermination, ResourceError> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        self.drain_until_root_exit(&mut state, pid)?;
        state.active.retain(|root| root.pid != pid);
        state.exited_roots.remove(&pid);
        let violations = signal.violations.load(Ordering::Acquire);
        Ok(if violations & MEMORY_VIOLATION != 0 {
            ProcessTermination::OutOfMemory
        } else if violations & PROCESS_VIOLATION != 0 {
            ProcessTermination::ProcessLimit
        } else {
            termination
        })
    }

    fn unregister_root(&self, pid: u32) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.active.retain(|root| root.pid != pid);
        state.exited_roots.remove(&pid);
    }

    fn drain_pending(&self, state: &mut RunState) -> Result<(), ResourceError> {
        loop {
            match self.next_notification(0)? {
                Some((message, pid)) => record_notification(state, message, pid),
                None => return Ok(()),
            }
        }
    }

    fn drain_until_root_exit(
        &self,
        state: &mut RunState,
        root_pid: u32,
    ) -> Result<(), ResourceError> {
        if state.exited_roots.contains(&root_pid) {
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
            let timeout_ms = remaining.as_millis().min(u128::from(u32::MAX)) as u32;
            match self.next_notification(timeout_ms.max(1))? {
                Some((message, pid)) => {
                    record_notification(state, message, pid);
                    if state.exited_roots.contains(&root_pid) {
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
                &mut message,
                &mut key,
                &mut overlapped,
                timeout_ms,
            )
        };
        if ok != 0 {
            return Ok(Some((message, overlapped as usize as u32)));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(WAIT_TIMEOUT as i32) {
            Ok(None)
        } else {
            Err(ResourceError::io("read Job Object notification", error))
        }
    }
}

fn record_notification(state: &mut RunState, message: u32, pid: u32) {
    match message {
        JOB_OBJECT_MSG_JOB_MEMORY_LIMIT => mark_active(state, MEMORY_VIOLATION),
        JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT => mark_active(state, PROCESS_VIOLATION),
        JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO => {
            state
                .exited_roots
                .extend(state.active.iter().map(|root| root.pid));
        }
        JOB_OBJECT_MSG_EXIT_PROCESS if state.active.iter().any(|root| root.pid == pid) => {
            state.exited_roots.insert(pid);
        }
        _ => {}
    }
}

fn mark_active(state: &mut RunState, violation: u8) {
    state.active.retain(|root| {
        if let Some(signal) = root.signal.upgrade() {
            // Aggregate Job notifications do not identify a culprit. Mark only roots active at
            // receipt as participants in a run-wide safety violation; future roots are unaffected.
            signal.violations.fetch_or(violation, Ordering::AcqRel);
            true
        } else {
            false
        }
    });
}

#[derive(Debug)]
pub(crate) struct WindowsSupervisor {
    run: Arc<WindowsRunJob>,
    root_job: OwnedHandle,
    signal: Arc<RootSignal>,
    pid: Option<u32>,
    terminated: bool,
    attach_fault: AttachFault,
}

impl WindowsSupervisor {
    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), ResourceError> {
        self.pid = Some(self.run.attach_root(
            self.root_job.raw(),
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
        let pid = self.pid.ok_or(ResourceError::MissingProcessId)?;
        self.run.classify_root(pid, &self.signal, termination)
    }

    pub(crate) fn terminate(&mut self) -> Result<(), ResourceError> {
        if self.terminated {
            return Ok(());
        }
        terminate_job(self.root_job.raw(), "terminate nested root process job")?;
        self.terminated = true;
        if let Some(pid) = self.pid {
            self.run.unregister_root(pid);
        }
        Ok(())
    }
}

impl Drop for WindowsSupervisor {
    fn drop(&mut self) {
        let _ = self.terminate();
        if let Some(pid) = self.pid {
            self.run.unregister_root(pid);
        }
    }
}

#[derive(Debug)]
struct OwnedHandle(isize);

impl OwnedHandle {
    fn new(handle: HANDLE, operation: &'static str) -> Result<Self, ResourceError> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(ResourceError::io(operation, io::Error::last_os_error()))
        } else {
            Ok(Self(handle as isize))
        }
    }

    fn raw(&self) -> HANDLE {
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
            information as *const _ as *const c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        Err(ResourceError::io(operation, io::Error::last_os_error()))
    } else {
        Ok(())
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
            &association as *const _ as *const c_void,
            size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
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

fn open_process(pid: u32) -> Result<OwnedHandle, ResourceError> {
    // SAFETY: pid comes from the just-spawned child and the handle is owned on success.
    OwnedHandle::new(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE,
                0,
                pid,
            )
        },
        "open suspended root process",
    )
}

fn assign_process(
    job: HANDLE,
    process: HANDLE,
    operation: &'static str,
) -> Result<(), ResourceError> {
    // SAFETY: job and process are live handles with assignment rights.
    if unsafe { AssignProcessToJobObject(job, process) } == 0 {
        Err(ResourceError::io(operation, io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

fn resume_primary_thread(pid: u32) -> Result<(), ResourceError> {
    // SAFETY: snapshot handle is validated and owned by the guard.
    let snapshot = OwnedHandle::new(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) },
        "snapshot suspended process threads",
    )?;
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    // SAFETY: entry has the documented size and remains writable through enumeration.
    if unsafe { Thread32First(snapshot.raw(), &mut entry) } == 0 {
        return Err(ResourceError::io(
            "enumerate suspended process threads",
            io::Error::last_os_error(),
        ));
    }
    loop {
        if entry.th32OwnerProcessID == pid {
            // SAFETY: thread id came from a live snapshot entry.
            let thread = OwnedHandle::new(
                unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) },
                "open suspended primary thread",
            )?;
            // SAFETY: thread is the sole primary thread of a CREATE_SUSPENDED process.
            if unsafe { ResumeThread(thread.raw()) } == u32::MAX {
                return Err(ResourceError::io(
                    "resume suspended primary thread",
                    io::Error::last_os_error(),
                ));
            }
            return Ok(());
        }
        // SAFETY: entry remains valid and has unchanged dwSize.
        if unsafe { Thread32Next(snapshot.raw(), &mut entry) } == 0 {
            break;
        }
    }
    Err(ResourceError::io(
        "find suspended primary thread",
        io::Error::new(
            io::ErrorKind::NotFound,
            "suspended primary thread not found",
        ),
    ))
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
    use std::time::{Duration, Instant};

    use camino::{Utf8Path, Utf8PathBuf};
    use hoimin_core::{
        CommandArg, EffectFailure, EffectId, ProcessLimits, RawRunLimits, RunLimits, RunProcess,
    };

    use super::{AttachFault, WindowsBackend};
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
    async fn assign_and_resume_failures_kill_suspended_root_before_user_code() {
        for (sequence, fault) in [AttachFault::Assign, AttachFault::Resume]
            .into_iter()
            .enumerate()
        {
            let temporary = tempfile::tempdir().unwrap();
            let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
            let marker = output_dir.join(format!("fault-{sequence}.marker"));
            let handler = ProcessHandler::new(
                ResourceBackend::Windows(
                    WindowsBackend::with_test_fault(&run_limits(), fault).unwrap(),
                ),
                output_dir.to_owned(),
            );
            let request = RunProcess {
                id: EffectId(200 + sequence as u64),
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
        }
    }

    #[tokio::test]
    async fn timed_out_root_is_unregistered_from_run_state() {
        let temporary = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
        let backend = WindowsBackend::new(&run_limits()).unwrap();
        let handler = ProcessHandler::new(
            ResourceBackend::Windows(backend.clone()),
            output_dir.to_owned(),
        );
        let request = RunProcess {
            id: EffectId(250),
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
            .unwrap_or_else(|error| error.into_inner());
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
                .unwrap_or_else(|error| error.into_inner());
            assert!(state.closed, "failed close still rejects future spawn");
            assert!(!state.terminated, "OS termination has not succeeded yet");
        }

        backend.close().expect("second close retries termination");
        let state = backend
            .inner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert!(state.closed);
        assert!(state.terminated);
    }
}
