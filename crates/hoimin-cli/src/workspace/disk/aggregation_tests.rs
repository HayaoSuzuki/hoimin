use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::num::NonZeroU64;
use std::sync::Mutex;
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{DiskFailureReason, DiskPolicy};
use serde::Deserialize;

use super::{
    AvailableSpace, DiskMeter, DiskMonitorStats, FilesystemKey, RootCapability,
    sample_meter_with_refresh_at,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootSpec {
    filesystem: u64,
    available: u64,
    files: Vec<(u64, u64)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    owned_bytes: u64,
    reasons: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MeterCase {
    schema: u8,
    mode: String,
    id: String,
    roots: Vec<RootSpec>,
    max_owned: NonZeroU64,
    min_free: NonZeroU64,
    expected_capacities: Vec<(u64, u64)>,
    expected_minimum: Option<u64>,
    expected_unix: Expected,
    expected_windows: Expected,
}

#[derive(Default)]
struct ScriptedSpace {
    by_root: BTreeMap<Utf8PathBuf, (u64, u64)>,
    queries: Mutex<Vec<(u64, u64)>>,
}

impl AvailableSpace for ScriptedSpace {
    fn available(&self, root: &RootCapability) -> io::Result<u64> {
        let capacity = self.by_root[&root.display_path];
        self.queries.lock().unwrap().push(capacity);
        Ok(capacity.1)
    }

    fn filesystem_key(&self, root: &RootCapability) -> io::Result<FilesystemKey> {
        Ok(FilesystemKey(self.by_root[&root.display_path].0))
    }
}

fn meter_for_case(directory: &Utf8Path, specs: &[RootSpec]) -> DiskMeter<ScriptedSpace> {
    let mut roots = Vec::new();
    let mut space = ScriptedSpace::default();
    let mut physical_files = BTreeMap::<(u64, u64), Utf8PathBuf>::new();
    for (index, spec) in specs.iter().enumerate() {
        let root = directory.join(format!("root-{index}"));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        for (index, &(file, length)) in spec.files.iter().enumerate() {
            let path = nested.join(format!("file-{index}"));
            match physical_files.entry((spec.filesystem, file)) {
                std::collections::btree_map::Entry::Occupied(original) => {
                    assert_eq!(fs::metadata(original.get()).unwrap().len(), length);
                    fs::hard_link(original.get(), path).unwrap();
                }
                std::collections::btree_map::Entry::Vacant(slot) => {
                    fs::File::create(&path).unwrap().set_len(length).unwrap();
                    slot.insert(path);
                }
            }
        }
        roots.push(RootCapability::open(&root).unwrap());
        space
            .by_root
            .insert(root, (spec.filesystem, spec.available));
    }
    DiskMeter::new(roots, space)
}

fn check_meter_case(case: &MeterCase) {
    let directory = tempfile::tempdir().unwrap();
    let meter = meter_for_case(Utf8Path::from_path(directory.path()).unwrap(), &case.roots);
    let expected = if cfg!(windows) {
        &case.expected_windows
    } else {
        &case.expected_unix
    };
    let reading = meter.measure().unwrap();
    assert_eq!(reading.owned_bytes, expected.owned_bytes, "{}", case.id);
    assert_eq!(reading.conservative_entries, cfg!(windows), "{}", case.id);
    let expected_capacities: BTreeMap<_, _> = case
        .expected_capacities
        .iter()
        .copied()
        .map(|(filesystem, available)| (FilesystemKey(filesystem), available))
        .collect();
    assert_eq!(
        reading.available_by_filesystem, expected_capacities,
        "{}",
        case.id
    );
    assert_eq!(
        *meter.space.queries.lock().unwrap(),
        case.expected_capacities,
        "{}",
        case.id
    );

    let (stop, _) = tokio::sync::watch::channel(None);
    let stats = Mutex::new(DiskMonitorStats::default());
    let failure = sample_meter_with_refresh_at(
        &meter,
        DiskPolicy {
            max_owned_bytes: case.max_owned,
            min_free_bytes: case.min_free,
        },
        &mut None,
        &stop,
        &stats,
        Instant::now(),
        &|| Ok(()),
    );
    let stats = stats.into_inner().unwrap();
    assert_eq!(
        stats.minimum_available_bytes(),
        case.expected_minimum,
        "{}",
        case.id
    );
    let reasons = failure.as_ref().map_or_else(Vec::new, |failure| {
        match &failure.reason {
            DiskFailureReason::WorkspaceSizeExceeded(observation)
            | DiskFailureReason::FilesystemReserveReached(observation) => {
                assert_eq!(observation.owned_bytes, expected.owned_bytes, "{}", case.id);
                assert_eq!(
                    Some(observation.available_bytes),
                    case.expected_minimum,
                    "{}",
                    case.id
                );
            }
            DiskFailureReason::MeasurementFailed(message) => {
                assert_eq!(
                    message, "disk measurement returned no filesystem capacity",
                    "{}",
                    case.id
                );
            }
            DiskFailureReason::ProcessFailed(message) => {
                panic!("{}: unexpected process failure {message}", case.id)
            }
        }
        std::iter::once(failure.code())
            .chain(
                failure
                    .secondary
                    .iter()
                    .map(hoimin_core::DiskSecondary::code),
            )
            .collect::<Vec<_>>()
    });
    assert_eq!(reasons, expected.reasons, "{}", case.id);
    assert_eq!(
        *meter.space.queries.lock().unwrap(),
        case.expected_capacities.repeat(2),
        "{} monitor measurement must query each filesystem once",
        case.id
    );
}

#[test]
fn native_disk_meter_and_stop_policy_match_lean() {
    let cases = include_str!("../../../../../formal/HoiminOracle/corpus/disk-meter.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<MeterCase>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 13);
    let mut ids = std::collections::BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        assert!(ids.insert(&case.id));
        check_meter_case(case);
    }
}
