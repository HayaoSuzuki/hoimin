use std::io;
use std::mem::size_of;
use std::os::windows::process::CommandExt;

use tokio::process::{Child, Command};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_QUOTA, PROCESS_TERMINATE, ResumeThread, THREAD_SUSPEND_RESUME,
};

use super::{ResourceError, windows::OwnedHandle};

pub(super) fn configure(command: &mut Command) {
    command.as_std_mut().creation_flags(CREATE_SUSPENDED);
}

pub(super) struct SuspendedChild {
    pid: u32,
    process: OwnedHandle,
}

impl SuspendedChild {
    pub(super) fn open(child: &Child) -> Result<Self, ResourceError> {
        let pid = child.id().ok_or(ResourceError::MissingProcessId)?;
        // SAFETY: pid comes from the just-spawned child and the handle is owned on success.
        let process = OwnedHandle::new(
            unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE,
                    0,
                    pid,
                )
            },
            "open suspended root process",
        )?;
        Ok(Self { pid, process })
    }

    pub(super) fn pid(&self) -> u32 {
        self.pid
    }

    pub(super) fn assign(&self, job: HANDLE, operation: &'static str) -> Result<(), ResourceError> {
        // SAFETY: job and process are live handles with assignment rights.
        if unsafe { AssignProcessToJobObject(job, self.process.raw()) } == 0 {
            Err(ResourceError::io(operation, io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    pub(super) fn resume(&self) -> Result<(), ResourceError> {
        resume_primary_thread(self.pid)
    }

    #[cfg(not(test))]
    pub(super) fn into_process_handle(self) -> OwnedHandle {
        self.process
    }
}

fn resume_primary_thread(pid: u32) -> Result<(), ResourceError> {
    // SAFETY: snapshot handle is validated and owned by the guard.
    let snapshot = OwnedHandle::new(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) },
        "snapshot suspended process threads",
    )?;
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>())
            .expect("Windows thread entry size fits u32"),
        ..Default::default()
    };
    // SAFETY: entry has the documented size and remains writable through enumeration.
    if unsafe { Thread32First(snapshot.raw(), &raw mut entry) } == 0 {
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
        if unsafe { Thread32Next(snapshot.raw(), &raw mut entry) } == 0 {
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
