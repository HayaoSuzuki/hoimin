use hoimin_cli::plan::{
    PLAN_SCHEMA_VERSION, PlanManifest, RankedPlanCandidate, RankingReason, RankingReasonCode,
};

#[path = "support/plan_schema_fixture.rs"]
mod plan_fixture;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

// Replacing descriptor admission with a blocking open must fail these public
// command tests within a deadline, without leaving a writerless FIFO child alive.
struct ReapedChild(Child);

impl Drop for ReapedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn bounded_output(command: Command) -> Output {
    bounded_output_with_timeout(command, Duration::from_secs(5))
}

fn bounded_output_with_timeout(mut command: Command, timeout: Duration) -> Output {
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().unwrap())
        .stderr(stderr.try_clone().unwrap());
    let mut child = ReapedChild(command.spawn().unwrap());
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "artifact input command exceeded the {timeout:?} deadline: {command:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    stdout.seek(SeekFrom::Start(0)).unwrap();
    stderr.seek(SeekFrom::Start(0)).unwrap();
    let mut output = Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    stdout.read_to_end(&mut output.stdout).unwrap();
    stderr.read_to_end(&mut output.stderr).unwrap();
    output
}

fn verify(path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .arg("verify")
        .arg(path)
        .args(["--top", "1", "--dry-run"]);
    command
}

fn progress(path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command.arg("progress").arg(path).arg(path);
    command
}

fn directory(root: &Path) -> PathBuf {
    let path = root.join("directory");
    std::fs::create_dir(&path).unwrap();
    path
}

fn missing(root: &Path) -> PathBuf {
    root.join("missing")
}

#[cfg(unix)]
fn fifo(root: &Path) -> PathBuf {
    let path = root.join("writerless-fifo");
    let output = Command::new("mkfifo").arg(&path).output().unwrap();
    assert!(output.status.success(), "mkfifo failed: {output:?}");
    path
}

#[cfg(unix)]
fn fifo_symlink(root: &Path) -> PathBuf {
    let target = fifo(root);
    symlink(&target, root)
}

#[cfg(unix)]
fn symlink(target: &Path, root: &Path) -> PathBuf {
    let path = root.join("input-link");
    std::os::unix::fs::symlink(target, &path).unwrap();
    path
}

macro_rules! rejected_input {
    ($name:ident, $command:ident, $fixture:ident, $diagnostic:literal) => {
        #[test]
        fn $name() {
            let root = tempfile::tempdir().unwrap();
            let path = $fixture(root.path());

            let output = bounded_output($command(&path));

            assert_eq!(output.status.code(), Some(2), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(stderr.contains($diagnostic), "{stderr}");
            assert!(stderr.contains(&path.display().to_string()), "{stderr}");
        }
    };
}

rejected_input!(
    verify_rejects_directory,
    verify,
    directory,
    "plan.manifest.invalid"
);
rejected_input!(
    progress_rejects_directory,
    progress,
    directory,
    "could not read report"
);
rejected_input!(
    verify_rejects_missing,
    verify,
    missing,
    "plan.manifest.invalid"
);
rejected_input!(
    progress_rejects_missing,
    progress,
    missing,
    "could not read report"
);
#[cfg(unix)]
rejected_input!(
    verify_rejects_writerless_fifo,
    verify,
    fifo,
    "not a regular file"
);
#[cfg(unix)]
rejected_input!(
    verify_rejects_writerless_fifo_symlink,
    verify,
    fifo_symlink,
    "not a regular file"
);
#[cfg(unix)]
rejected_input!(
    progress_rejects_writerless_fifo,
    progress,
    fifo,
    "not a regular file"
);
#[cfg(unix)]
rejected_input!(
    progress_rejects_writerless_fifo_symlink,
    progress,
    fifo_symlink,
    "not a regular file"
);

fn regular_manifest(root: &Path) -> PathBuf {
    let path = root.join("plan.json");
    // Keep the shape current: unsupported-version checks happen only after a
    // complete typed decode, including any large trailing whitespace.
    let mut manifest = plan_fixture::manifest(1, 32);
    manifest.schema_version = 0;
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    path
}

#[cfg(unix)]
fn manifest_symlink(root: &Path) -> PathBuf {
    let target = regular_manifest(root);
    symlink(&target, root)
}

macro_rules! parsed_manifest {
    ($name:ident, $fixture:ident) => {
        parsed_manifest!($name, $fixture, Duration::from_secs(5));
    };
    ($name:ident, $fixture:ident, $timeout:expr) => {
        #[test]
        fn $name() {
            let root = tempfile::tempdir().unwrap();
            let path = $fixture(root.path());

            let output = bounded_output_with_timeout(verify(&path), $timeout);

            assert_eq!(output.status.code(), Some(2), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(stderr.contains("unsupported schema version 0"), "{stderr}");
        }
    };
}

parsed_manifest!(verify_parses_regular_manifest, regular_manifest);
#[cfg(unix)]
parsed_manifest!(verify_parses_regular_manifest_symlink, manifest_symlink);

fn report(root: &Path, golden: &str) -> PathBuf {
    let path = root.join("report");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(golden);
    std::fs::copy(source, &path).unwrap();
    path
}

fn jsonl_report(root: &Path) -> PathBuf {
    report(root, "events/schema-v10-current.jsonl")
}

#[cfg(unix)]
fn jsonl_report_symlink(root: &Path) -> PathBuf {
    let target = jsonl_report(root);
    symlink(&target, root)
}

macro_rules! accepted_report {
    ($name:ident, $fixture:ident) => {
        accepted_report!($name, $fixture, Duration::from_secs(5));
    };
    ($name:ident, $fixture:ident, $timeout:expr) => {
        #[test]
        fn $name() {
            let root = tempfile::tempdir().unwrap();
            let path = $fixture(root.path());

            let output = bounded_output_with_timeout(progress(&path), $timeout);

            assert!(output.status.success(), "{output:?}");
            let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(document["inputs"][0]["usable"], true);
            assert_eq!(document["inputs"][1]["usable"], true);
            assert_eq!(document["comparisons"][0]["state"], "stalled");
        }
    };
}

accepted_report!(progress_accepts_regular_jsonl, jsonl_report);
#[cfg(unix)]
accepted_report!(progress_accepts_jsonl_symlink, jsonl_report_symlink);

fn pad_beyond_review_limit(path: &Path) {
    let mut file = File::options().append(true).open(path).unwrap();
    let block = vec![b' '; 1024 * 1024];
    for _ in 0..65 {
        file.write_all(&block).unwrap();
    }
}

fn large_manifest(root: &Path) -> PathBuf {
    let path = regular_manifest(root);
    pad_beyond_review_limit(&path);
    path
}

fn large_report(root: &Path) -> PathBuf {
    let path = jsonl_report(root);
    pad_beyond_review_limit(&path);
    path
}

// These finite regular files exceed 64 MiB; progress reads its report twice.
// Streaming JSON decoding in debug builds can legitimately exceed the short
// FIFO-open watchdog on a shared runner. Keep that watchdog for the small/FIFO
// cases, while separately bounding large-input work without a throughput claim.
const LARGE_REGULAR_INPUT_TIMEOUT: Duration = Duration::from_secs(30);

parsed_manifest!(
    verify_does_not_apply_review_size_limit,
    large_manifest,
    LARGE_REGULAR_INPUT_TIMEOUT
);
accepted_report!(
    progress_does_not_apply_review_size_limit,
    large_report,
    LARGE_REGULAR_INPUT_TIMEOUT
);

macro_rules! closed_report_progress {
    ($name:ident, $index:expr) => {
        #[test]
        fn $name() {
            // Arrange: alter one raw event so duplicate keys survive until the reader.
            let root = tempfile::tempdir().unwrap();
            let mut events: Vec<serde_json::Value> =
                include_str!("golden/events/schema-v10-current.jsonl")
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
            events[$index]["unexpected"] = 123.into();
            let wire = {
                events
                    .iter()
                    .map(|event| serde_json::to_string(event).unwrap())
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let path = root.path().join("report");
            std::fs::write(&path, wire).unwrap();

            // Act.
            let output = bounded_output(progress(&path));

            // Assert.
            assert_eq!(output.status.code(), Some(2), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("unknown field `unexpected`"),
                "{output:?}"
            );
        }
    };
}

closed_report_progress!(progress_rejects_jsonl_unknown_run, 0);
closed_report_progress!(progress_rejects_jsonl_unknown_baseline, 1);
closed_report_progress!(progress_rejects_jsonl_unknown_mutant_start, 2);
closed_report_progress!(progress_rejects_jsonl_unknown_mutant_finish, 3);
closed_report_progress!(progress_rejects_jsonl_unknown_diagnostic, 4);
closed_report_progress!(progress_rejects_jsonl_unknown_summary, 5);
