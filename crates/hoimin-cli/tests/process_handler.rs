use std::ffi::OsStr;
use std::fs;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::process::{ProcessCancellation, ProcessHandler, ProcessRequest};
use hoimin_cli::resource::{PortableBackend, ResourceBackend};
use hoimin_core::{
    CommandArg, EffectFailure, EffectId, ProcessLimits, ProcessTermination, ResourceMode,
    RunProcess,
};

#[cfg(unix)]
fn native_arg(value: &OsStr) -> CommandArg {
    use std::os::unix::ffi::OsStrExt;
    CommandArg::Unix(value.as_bytes().to_vec())
}

#[cfg(windows)]
fn native_arg(value: &OsStr) -> CommandArg {
    use std::os::windows::ffi::OsStrExt;
    CommandArg::Windows(value.encode_wide().collect())
}

fn utf8_arg(value: &str) -> CommandArg {
    native_arg(OsStr::new(value))
}

fn python_executable() -> CommandArg {
    if let Some(configured) = std::env::var_os("PYTHON") {
        return native_arg(&configured);
    }
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for candidate in [
        workspace.join(".venv/Scripts/python.exe"),
        workspace.join(".venv/bin/python"),
    ] {
        if candidate.is_file() {
            return native_arg(candidate.as_os_str());
        }
    }
    #[cfg(windows)]
    return utf8_arg("python");
    #[cfg(not(windows))]
    utf8_arg("python3")
}

fn limits(timeout: Duration, max_output_bytes: u64) -> ProcessLimits {
    ProcessLimits {
        timeout,
        max_output_bytes,
        max_memory_bytes: 256 * 1024 * 1024,
        max_processes: 8,
    }
}

fn run_python(id: u64, code: &str, limits: ProcessLimits) -> RunProcess {
    RunProcess {
        id: EffectId(id),
        argv: vec![python_executable(), utf8_arg("-c"), utf8_arg(code)],
        cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
        limits,
    }
}

fn portable_handler(output_dir: &Utf8Path) -> ProcessHandler {
    ProcessHandler::new(
        ResourceBackend::Portable(PortableBackend::for_tests()),
        output_dir.to_owned(),
    )
}

struct FixtureChildGuard {
    pid_file: Utf8PathBuf,
}

impl FixtureChildGuard {
    fn new(pid_file: Utf8PathBuf) -> Self {
        Self { pid_file }
    }

    fn pid(&self) -> Option<u32> {
        fs::read_to_string(&self.pid_file)
            .ok()
            .and_then(|value| value.trim().parse().ok())
    }
}

impl Drop for FixtureChildGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid().filter(|pid| process_exists(*pid)) {
            terminate_fixture_process(pid);
        }
    }
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    // SAFETY: signal 0 performs no mutation and accepts any numeric pid.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(unix)]
fn terminate_fixture_process(pid: u32) {
    // SAFETY: this is a best-effort test cleanup for the exact fixture pid.
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: the handle is checked and closed on every successful open.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0;
        let active =
            GetExitCodeProcess(handle, &mut exit_code) != 0 && exit_code == STILL_ACTIVE as u32;
        CloseHandle(handle);
        active
    }
}

#[cfg(windows)]
fn terminate_fixture_process(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    // SAFETY: the handle is checked, used only for fixture termination, and closed.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
}

async fn wait_until_process_stops(pid: u32) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while process_exists(pid) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    !process_exists(pid)
}

mod portable {
    use super::*;

    #[tokio::test]
    async fn output_is_drained_after_retention_cap() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = run_python(
            1,
            "import sys; sys.stdout.buffer.write(b'x' * 2000000); sys.stdout.flush()",
            limits(Duration::from_secs(5), 1024),
        );

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.output.retained, 1024);
        assert_eq!(event.output.observed, 2_000_000);
        assert!(event.output.observed > event.output.retained);
        assert_eq!(
            fs::metadata(handler.spool_path(&event.output).unwrap())
                .unwrap()
                .len(),
            1024
        );
    }

    #[tokio::test]
    async fn drains_stdout_and_stderr_under_combined_cap() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = run_python(
            2,
            "import sys; sys.stdout.buffer.write(b'o'*800); sys.stdout.flush(); sys.stderr.buffer.write(b'e'*800); sys.stderr.flush()",
            limits(Duration::from_secs(5), 1024),
        );

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.output.retained, 1024);
        assert_eq!(event.output.observed, 1600);
        assert_eq!(
            fs::read(handler.spool_path(&event.output).unwrap())
                .unwrap()
                .len(),
            1024
        );
    }

    #[tokio::test]
    async fn spools_non_utf8_output() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = run_python(
            3,
            "import sys; sys.stdout.buffer.write(bytes([255, 254, 0, 128])); sys.stdout.flush()",
            limits(Duration::from_secs(5), 64),
        );

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.output.retained, 4);
        assert_eq!(event.output.observed, 4);
        assert_eq!(
            fs::read(handler.spool_path(&event.output).unwrap()).unwrap(),
            [255, 254, 0, 128]
        );
    }

    #[tokio::test]
    async fn returns_zero_and_nonzero_exit() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());

        let zero = handler
            .handle(run_python(
                4,
                "raise SystemExit(0)",
                limits(Duration::from_secs(5), 64),
            ))
            .await
            .unwrap();
        let nonzero = handler
            .handle(run_python(
                5,
                "raise SystemExit(7)",
                limits(Duration::from_secs(5), 64),
            ))
            .await
            .unwrap();

        assert_eq!(zero.id, EffectId(4));
        assert_eq!(zero.termination, ProcessTermination::Exit(0));
        assert_eq!(zero.resource_mode, ResourceMode::BestEffort);
        assert_eq!(nonzero.id, EffectId(5));
        assert_eq!(nonzero.termination, ProcessTermination::Exit(7));
    }

    #[tokio::test]
    async fn passes_native_arguments_without_a_shell() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let expected = "space & | $() ; literal";
        let request = RunProcess {
            id: EffectId(51),
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg("import os,sys; sys.stdout.buffer.write(os.fsencode(sys.argv[1]))"),
                utf8_arg(expected),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(5), 128),
        };

        let event = handler.handle(request).await.unwrap();

        assert_eq!(event.termination, ProcessTermination::Exit(0));
        assert_eq!(
            fs::read(handler.spool_path(&event.output).unwrap()).unwrap(),
            expected.as_bytes()
        );
    }

    #[tokio::test]
    async fn returns_cancelled() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let cancellation = ProcessCancellation::new();
        let request = ProcessRequest::from(run_python(
            6,
            "import time\nwhile True: time.sleep(1)",
            limits(Duration::from_secs(10), 64),
        ))
        .with_cancellation(cancellation.clone());

        let (event, ()) = tokio::join!(handler.run(request), async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancellation.cancel();
        });

        assert_eq!(event.unwrap().termination, ProcessTermination::Cancelled);
    }

    #[tokio::test]
    async fn maps_spawn_failure_to_effect_failed() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let request = RunProcess {
            id: EffectId(7),
            argv: vec![utf8_arg("definitely-missing-hoimin-executable")],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(1), 64),
        };

        let failed = handler.handle(request).await.unwrap_err();

        assert_eq!(failed.id, EffectId(7));
        assert!(matches!(
            failed.failure,
            EffectFailure::Io { ref code, ref operation, .. }
                if code == "process.spawn" && operation == "spawn process"
        ));
    }

    #[tokio::test]
    async fn honors_requested_timeout() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
        let timeout = Duration::from_millis(200);
        let started = Instant::now();

        let event = handler
            .handle(run_python(
                8,
                "import time\ntime.sleep(30)",
                limits(timeout, 64),
            ))
            .await
            .unwrap();

        assert_eq!(event.termination, ProcessTermination::Timeout);
        assert!(started.elapsed() >= timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(event.elapsed >= timeout);
    }

    #[tokio::test]
    async fn timeout_terminates_descendants() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("fixture-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler(output_dir);
        let request = RunProcess {
            id: EffectId(9),
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys,time; time.sleep(0.4); child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(1), 64),
        };

        let event = handler.handle(request).await.unwrap();
        let child_pid = guard.pid().expect("fixture child wrote its pid");

        assert_eq!(event.termination, ProcessTermination::Timeout);
        assert!(wait_until_process_stops(child_pid).await);
    }

    #[tokio::test]
    async fn cancellation_terminates_descendants() {
        let output = tempfile::tempdir().unwrap();
        let output_dir = Utf8Path::from_path(output.path()).unwrap();
        let pid_file = output_dir.join("cancelled-fixture-child.pid");
        let guard = FixtureChildGuard::new(pid_file.clone());
        let handler = portable_handler(output_dir);
        let cancellation = ProcessCancellation::new();
        let request = ProcessRequest::from(RunProcess {
            id: EffectId(10),
            argv: vec![
                python_executable(),
                utf8_arg("-c"),
                utf8_arg(
                    "import pathlib,subprocess,sys,time; time.sleep(0.2); child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)",
                ),
                native_arg(pid_file.as_std_path().as_os_str()),
            ],
            cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: limits(Duration::from_secs(10), 64),
        })
        .with_cancellation(cancellation.clone());

        let (event, ()) = tokio::join!(handler.run(request), async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            while !pid_file.exists() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            cancellation.cancel();
        });
        let child_pid = guard.pid().expect("fixture child wrote its pid");

        assert_eq!(event.unwrap().termination, ProcessTermination::Cancelled);
        assert!(wait_until_process_stops(child_pid).await);
    }

    #[test]
    fn portable_backend_reports_best_effort() {
        assert_eq!(
            PortableBackend::for_tests().mode(),
            ResourceMode::BestEffort
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn normal_linux_portable_backend_requires_explicit_opt_in() {
        let error = PortableBackend::new(false).unwrap_err();
        assert!(error.to_string().contains("--allow-best-effort-memory"));
        assert_eq!(
            PortableBackend::new(true).unwrap().mode(),
            ResourceMode::BestEffort
        );
    }
}
