#![cfg(windows)]

use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::JobObjects::{
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

struct ObservedProcess(HANDLE);

impl ObservedProcess {
    fn open(pid: &Value) -> Self {
        let pid = u32::try_from(pid.as_u64().expect("published process ID")).unwrap();
        // SAFETY: opening a synchronization handle does not modify the published process.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(
            !handle.is_null(),
            "open {pid}: {}",
            std::io::Error::last_os_error()
        );
        Self(handle)
    }

    fn state(&self) -> u32 {
        // SAFETY: this wrapper retains ownership until Drop.
        unsafe { WaitForSingleObject(self.0, 0) }
    }
}

impl Drop for ObservedProcess {
    fn drop(&mut self) {
        // SAFETY: the valid handle is owned exclusively by this wrapper.
        unsafe { CloseHandle(self.0) };
    }
}

fn python() -> std::path::PathBuf {
    // Use the base interpreter: Windows venv launchers can add another process to the job.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let configuration = fs::read_to_string(root.join(".venv/pyvenv.cfg")).unwrap();
    let home = configuration
        .lines()
        .find_map(|line| line.strip_prefix("home = "))
        .unwrap();
    let executable = Path::new(home).join("python.exe");
    assert!(executable.is_file(), "{}", executable.display());
    executable
}

async fn observe(path: &Path) -> Value {
    loop {
        match fs::read(path) {
            Ok(bytes) => return serde_json::from_slice(&bytes).unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("read {}: {error}", path.display()),
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn simultaneous_observation(
    shared: &Path,
    mode: &str,
    memory_bytes: u64,
    processes: u64,
) -> Vec<ObservedProcess> {
    let first_path = shared.join("first.json");
    let second_path = shared.join("second.json");
    let (first, second) = tokio::join!(observe(&first_path), observe(&second_path));
    assert_ne!(first["pid"], second["pid"], "distinct mutant roots");
    let mut handles = Vec::new();
    for record in [&first, &second] {
        assert_eq!(record["memory_limit"], memory_bytes);
        assert_eq!(record["process_limit"], processes);
        assert_eq!(
            record["flags"],
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_JOB_MEMORY
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        );
        assert_eq!(record["active_processes"], 2);
        if mode == "memory" {
            assert_eq!(record["payload_bytes"], 96 * 1024 * 1024);
            assert!(record["peak_job_memory"].as_u64().unwrap() >= 96 * 1024 * 1024);
            assert!(record["peak_job_memory"].as_u64().unwrap() < memory_bytes);
        }
        handles.push(ObservedProcess::open(&record["pid"]));
        handles.push(ObservedProcess::open(&record["child_pid"]));
    }
    // All four processes are live while both roots retain their payload and await release.
    for handle in &handles {
        assert_eq!(handle.state(), WAIT_TIMEOUT);
    }
    if mode == "memory" {
        assert!(
            first["payload_bytes"].as_u64().unwrap() + second["payload_bytes"].as_u64().unwrap()
                > memory_bytes
        );
    } else {
        assert!(
            first["active_processes"].as_u64().unwrap()
                + second["active_processes"].as_u64().unwrap()
                > processes
        );
    }
    println!("native {mode} simultaneous occupancy: {first}; {second}");
    fs::write(shared.join("release"), b"observed").unwrap();
    handles
}

async fn assert_scope(mode: &str, memory: &str, memory_bytes: u64, processes: u64) {
    let project = tempfile::tempdir().unwrap();
    let shared = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("target.py"),
        "first = 10 + 1\nsecond = 20 + 2\n",
    )
    .unwrap();
    fs::write(
        project.path().join("probe.py"),
        include_str!("support/windows_resource_scope.py"),
    )
    .unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["run", "--root"])
        .arg(project.path())
        .args([
            "--file",
            "target.py",
            "--operators",
            "binary_add_sub",
            "--jobs",
            "2",
            "--max-mutants",
            "2",
            "--max-memory",
            memory,
            "--max-processes",
        ])
        .arg(processes.to_string())
        .args([
            "--min-free-space",
            "1B",
            "--mutant-timeout",
            "40s",
            "--total-timeout",
            "90s",
            "--format",
            "json",
            "--",
        ])
        .arg(python())
        .args(["probe.py", mode])
        .arg(shared.path())
        .kill_on_drop(true);
    let running = command.output();
    tokio::pin!(running);
    let observation = simultaneous_observation(shared.path(), mode, memory_bytes, processes);
    let handles = tokio::select! {
        output = &mut running => panic!("CLI exited before simultaneous occupancy: {:?}", output.unwrap()),
        observed = tokio::time::timeout(Duration::from_secs(45), observation) => observed.expect("native roots failed to reach barrier"),
    };
    let output = tokio::time::timeout(Duration::from_secs(50), &mut running)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["baseline"]["termination"],
        serde_json::json!({"Exit": 0})
    );
    let configured = &report["run"]["normalized_config"]["limits"];
    assert_eq!(configured["jobs"], 2);
    assert_eq!(configured["max_memory"], memory_bytes);
    assert_eq!(configured["max_processes"], processes);
    let mutants = report["mutants"].as_array().expect("mutant results");
    assert_eq!(mutants.len(), 2);
    for mutant in mutants {
        assert_eq!(mutant["status"], "survived");
        assert_eq!(mutant["termination"], serde_json::json!({"Exit": 0}));
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if handles.iter().all(|handle| handle.state() == WAIT_OBJECT_0) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("root or descendant survived CLI cleanup");
}

#[tokio::test]
async fn cli_jobs_two_memory_caps_are_per_root_with_cleanup() {
    assert_scope("memory", "160MiB", 160 * 1024 * 1024, 8).await;
}

#[tokio::test]
async fn cli_jobs_two_process_caps_are_per_root_with_cleanup() {
    assert_scope("process", "256MiB", 256 * 1024 * 1024, 3).await;
}
