use super::*;
use crate::metrics::{Clock, MetricsOutput};
use hoimin_core::RunMetrics;
use std::path::Path;
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Timing {
    OnTime,
    ExpiresBeforeAdmission,
    ExpiresAfterAdmission,
}

#[derive(PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Work {
    Publish,
    NoCollector,
    RunFailed,
    InvalidMetrics,
    RenameFailed,
    WorkerPanicked,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileState {
    Previous,
    Published,
    Directory,
}

#[derive(serde::Deserialize)]
struct Observation {
    expired: bool,
    warnings: Vec<String>,
    at_return: FileState,
    settled: FileState,
}

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    id: String,
    mode: String,
    timing: Timing,
    work: Work,
    expected: Observation,
}

#[derive(Default)]
struct FixtureClock(Mutex<Option<OwnedStartHook>>);

impl Clock for FixtureClock {
    fn elapsed(&self) -> Duration {
        if let Some(pause) = self.0.lock().unwrap().take() {
            pause();
        }
        Duration::ZERO
    }
}

const GRACE: Duration = Duration::from_secs(10);
const OLD_CONTENTS: &[u8] = b"previous metrics bytes\n";

fn read_metrics(path: &Path) -> FileState {
    if path.is_dir() {
        assert_eq!(std::fs::read(path.join("previous")).unwrap(), OLD_CONTENTS);
        assert_eq!(std::fs::read_dir(path).unwrap().count(), 1);
        return FileState::Directory;
    }
    let bytes = std::fs::read(path).unwrap();
    if bytes == OLD_CONTENTS {
        return FileState::Previous;
    }

    assert_eq!(bytes.last(), Some(&b'\n'));
    let metrics: RunMetrics = serde_json::from_slice(&bytes).unwrap();
    metrics.validate().unwrap();
    let mut expected = RunMetrics::empty("new-run");
    expected.discovered = 2;
    assert_eq!(metrics, expected);
    FileState::Published
}

async fn wait_for_worker(parent: std::sync::Weak<cap_std::fs::Dir>) {
    tokio::time::timeout(Duration::from_secs(5), async move {
        while parent.strong_count() != 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("blocking metrics worker must release its output directory");
}

async fn replay(case: Case) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("metrics.json");
    if case.work == Work::RenameFailed {
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("previous"), OLD_CONTENTS).unwrap();
    } else {
        std::fs::write(&path, OLD_CONTENTS).unwrap();
    }
    let parent = Arc::new(
        cap_std::fs::Dir::open_ambient_dir(directory.path(), cap_std::ambient_authority()).unwrap(),
    );
    let worker_parent = Arc::downgrade(&parent);
    let output = MetricsOutput::from_parent(parent, path.clone()).unwrap();

    let (entered_tx, mut entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let pause: OwnedStartHook = Box::new(move || {
        entered_tx.send(()).unwrap();
        release_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("metrics fixture must release the paused worker");
    });
    let mut clock = FixtureClock::default();
    let before_start: OwnedStartHook = match case.timing {
        Timing::ExpiresBeforeAdmission => pause,
        Timing::ExpiresAfterAdmission => {
            clock = FixtureClock(Mutex::new(Some(pause)));
            Box::new(|| {})
        }
        Timing::OnTime if case.work == Work::WorkerPanicked => {
            Box::new(|| panic!("fixture metrics worker panicked"))
        }
        Timing::OnTime => Box::new(|| {}),
    };
    let run_id = if case.work == Work::InvalidMetrics {
        ""
    } else {
        "new-run"
    };
    let collector =
        (case.work != Work::NoCollector).then(|| MetricsCollector::with_clock(run_id, clock));
    let run_failure = (case.work == Work::RunFailed).then(|| "fixture run failed".to_owned());
    let budget = ShutdownBudget::after_observation_with_grace(
        ShutdownCause::Failure,
        tokio::time::Instant::now(),
        GRACE,
    );
    let mut finalize = Box::pin(finalize_metrics_with_shutdown(
        output,
        collector,
        run_failure,
        2,
        0,
        vec![("previous", "keep previous warning".into())],
        &budget,
        before_start,
    ));
    if case.timing != Timing::OnTime {
        tokio::select! {
            result = &mut finalize => panic!("{} returned before the worker paused: {:?}", case.id, result.expiry),
            entered = &mut entered_rx => entered.unwrap(),
        }
        advance_test_clock(GRACE + Duration::from_millis(1)).await;
    }
    let result = finalize.await;
    assert_eq!(result.expiry.is_some(), case.expected.expired, "{}", case.id);
    assert_eq!(
        result.warnings.first(),
        Some(&("previous", "keep previous warning".into())),
        "{}",
        case.id
    );
    let codes: Vec<_> = result.warnings.iter().map(|(code, _)| *code).collect();
    assert_eq!(codes, case.expected.warnings, "{}: {:?}", case.id, result.warnings);
    assert_eq!(read_metrics(&path), case.expected.at_return, "{}", case.id);

    if case.timing != Timing::OnTime {
        release_tx.send(()).unwrap();
    }
    wait_for_worker(worker_parent).await;
    assert_eq!(read_metrics(&path), case.expected.settled, "{}", case.id);
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        1,
        "{} leaked an unpublished metrics file",
        case.id
    );
}

#[tokio::test]
async fn metrics_publication_across_shutdown_deadline_matches_lean() {
    let mut ids = BTreeSet::new();
    for row in include_str!("../../../formal/HoiminOracle/corpus/shutdown-metrics.jsonl").lines() {
        let case: Case = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(ids.insert(case.id.clone()));
        replay(case).await;
    }
    assert_eq!(ids.len(), 10);
}
