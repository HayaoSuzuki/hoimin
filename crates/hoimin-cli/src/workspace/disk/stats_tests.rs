use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use proptest::prelude::*;
use serde::Deserialize;

use super::{
    DiskMeasurement, DiskMonitorStats, FilesystemKey, MeterReading, sample_meter_with_refresh_at,
};

struct ScriptedMeter(Mutex<VecDeque<io::Result<MeterReading>>>);

impl DiskMeasurement for ScriptedMeter {
    fn measure(&self) -> io::Result<MeterReading> {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted reading")
    }
}

fn reading(capacities: BTreeMap<u64, u64>, owned: u64) -> MeterReading {
    MeterReading {
        owned_bytes: owned,
        available_by_filesystem: capacities
            .into_iter()
            .map(|(key, value)| (FilesystemKey(key), value))
            .collect(),
        conservative_entries: false,
        elapsed: Duration::from_millis(owned),
    }
}

struct Samples {
    stats: Mutex<DiskMonitorStats>,
    stop: tokio::sync::watch::Sender<Option<hoimin_core::DiskFailure>>,
}

impl Samples {
    fn new() -> Self {
        let (stop, _) = tokio::sync::watch::channel(None);
        Self {
            stats: Mutex::new(DiskMonitorStats::default()),
            stop,
        }
    }

    fn accept(&self, capacities: BTreeMap<u64, u64>, owned: u64) {
        self.sample(Ok(reading(capacities, owned)), false);
    }

    fn sample(&self, reading: io::Result<MeterReading>, refresh_fails: bool) {
        let meter = ScriptedMeter(Mutex::new(VecDeque::from([reading])));
        let policy = hoimin_core::DiskPolicy {
            max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
            min_free_bytes: std::num::NonZeroU64::new(500).unwrap(),
        };
        sample_meter_with_refresh_at(
            &meter,
            policy,
            &mut None,
            &self.stop,
            &self.stats,
            Instant::now(),
            &|| {
                if refresh_fails {
                    Err(io::Error::other("heartbeat failed"))
                } else {
                    Ok(())
                }
            },
        );
    }

    fn snapshot(&self) -> DiskMonitorStats {
        self.stats.lock().unwrap().clone()
    }
}

#[rstest::rstest]
#[case::vec_1(0)]
#[case::vec_btreemap_from_1_0_2(1)]
#[case::vec_btreemap_from_1_100_2_50_btreemap_from_1_90_3(2)]
#[case::vec_btreemap_from_2_50_btreemap_from_1_90_btreemap_from_2_80_4(3)]
#[case::vec_btreemap_from_2_50_btreemap_from_1_90_btreemap_from_2_30_5(4)]
fn historical_minimum_preserves_unmeasured_zero_and_missing_filesystems(
    #[case] scenario_index: usize,
) {
    let cases = [
        (vec![], None),
        (vec![BTreeMap::from([(1, 0)])], Some(0)),
        (
            vec![
                BTreeMap::from([(1, 100), (2, 50)]),
                BTreeMap::from([(1, 90)]),
            ],
            Some(50),
        ),
        (
            vec![
                BTreeMap::from([(2, 50)]),
                BTreeMap::from([(1, 90)]),
                BTreeMap::from([(2, 80)]),
            ],
            Some(50),
        ),
        (
            vec![
                BTreeMap::from([(2, 50)]),
                BTreeMap::from([(1, 90)]),
                BTreeMap::from([(2, 30)]),
            ],
            Some(30),
        ),
    ];

    let (history, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let samples = Samples::new();

    for capacities in history {
        samples.accept(capacities, 1);
    }

    let actual = samples.snapshot().minimum_available_bytes();

    assert_eq!(actual, expected);
}

proptest! {
    #[test]
    fn historical_minimum_matches_all_accepted_raw_capacities(
        history in prop::collection::vec(prop::collection::btree_map(0_u64..4, any::<u64>(), 1..=4), 0..24),
    ) {
        let samples = Samples::new();
        let expected = history.iter().flat_map(BTreeMap::values).copied().min();

        for capacities in history {
            samples.accept(capacities, 1);
        }

        let actual = samples.snapshot().minimum_available_bytes();

        prop_assert_eq!(actual, expected);
    }
}

#[rstest::rstest]
#[case::refresh_failure(true, || Ok(reading(BTreeMap::from([(1, 0)]), 99)))]
#[case::measurement_failure(false, || Err(io::Error::other("measurement failed")))]
#[case::missing_capacity(false, || Ok(reading(BTreeMap::new(), 99)))]
fn rejected_samples_preserve_all_successful_statistics(
    #[values(vec![], vec![BTreeMap::from([(1, 900), (2, 700)])])] initial: Vec<BTreeMap<u64, u64>>,
    #[case] refresh_fails: bool,
    #[case] rejected: fn() -> io::Result<MeterReading>,
) {
    let samples = Samples::new();
    for capacities in initial {
        samples.accept(capacities, 1);
    }
    let before = samples.snapshot();
    let rejected = rejected();

    samples.sample(rejected, refresh_fails);
    let after = samples.snapshot();

    assert_eq!(after, before);
}

#[test]
fn valid_samples_update_statistics_even_after_a_sticky_stop() {
    let samples = Samples::new();
    let history = [(400, 3), (800, 7), (30, 5)];

    for (available, owned) in history {
        samples.accept(BTreeMap::from([(1, available)]), owned);
    }
    let stats = samples.snapshot();

    assert_eq!(stats.sample_count, 3);
    assert_eq!(stats.peak_owned_bytes, 7);
    assert_eq!(stats.maximum_measurement, Duration::from_millis(7));
    assert_eq!(stats.minimum_available_bytes(), Some(30));
    let filesystem = stats.filesystems.get(&FilesystemKey(1)).unwrap();
    assert_eq!(filesystem.start, 400);
    assert_eq!(filesystem.minimum, 30);
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StatisticsSample {
    Reading {
        owned_bytes: u64,
        elapsed_ms: u64,
        capacities: Vec<(u64, u64)>,
    },
    MeasurementFailed,
    RefreshFailed,
}

impl StatisticsSample {
    fn replay(&self, samples: &Samples) {
        match self {
            Self::Reading {
                owned_bytes,
                elapsed_ms,
                capacities,
            } => {
                let mut value = reading(capacities.iter().copied().collect(), *owned_bytes);
                value.elapsed = Duration::from_millis(*elapsed_ms);
                samples.sample(Ok(value), false);
            }
            Self::MeasurementFailed => {
                samples.sample(Err(io::Error::other("measurement failed")), false);
            }
            Self::RefreshFailed => {
                samples.sample(Ok(reading(BTreeMap::from([(99, 0)]), u64::MAX)), true);
            }
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FilesystemObservation {
    start: u64,
    minimum: u64,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StatisticsObservation {
    sample_count: u64,
    peak_owned_bytes: u64,
    maximum_measurement_ms: u64,
    minimum_available_bytes: Option<u64>,
    filesystems: BTreeMap<u64, FilesystemObservation>,
}

impl From<DiskMonitorStats> for StatisticsObservation {
    fn from(stats: DiskMonitorStats) -> Self {
        Self {
            sample_count: stats.sample_count,
            peak_owned_bytes: stats.peak_owned_bytes,
            maximum_measurement_ms: u64::try_from(stats.maximum_measurement.as_millis()).unwrap(),
            minimum_available_bytes: stats.minimum_available_bytes(),
            filesystems: stats
                .filesystems
                .into_iter()
                .map(|(key, stats)| {
                    (
                        key.0,
                        FilesystemObservation {
                            start: stats.start,
                            minimum: stats.minimum,
                        },
                    )
                })
                .collect(),
        }
    }
}

#[test]
fn disk_statistics_match_lean_after_every_sample() {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Case {
        schema: u8,
        mode: String,
        id: String,
        samples: Vec<StatisticsSample>,
        expected: Vec<StatisticsObservation>,
    }

    let cases = include_str!("../../../../../formal/HoiminOracle/corpus/disk-statistics.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<Case>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 10);
    let mut ids = std::collections::BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        assert!(ids.insert(&case.id));
        assert_eq!(case.expected.len(), case.samples.len() + 1);
        let samples = Samples::new();
        assert_eq!(
            StatisticsObservation::from(samples.snapshot()),
            case.expected[0],
            "{} initial",
            case.id
        );
        for (index, (sample, expected)) in case.samples.iter().zip(&case.expected[1..]).enumerate()
        {
            sample.replay(&samples);
            let actual = StatisticsObservation::from(samples.snapshot());
            assert_eq!(&actual, expected, "{} after sample {}", case.id, index + 1);
        }
    }
}
