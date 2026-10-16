use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use proptest::prelude::*;

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

#[test]
fn historical_minimum_preserves_unmeasured_zero_and_missing_filesystems() {
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
    for (history, expected) in cases {
        let samples = Samples::new();

        for capacities in history {
            samples.accept(capacities, 1);
        }
        let actual = samples.snapshot().minimum_available_bytes();

        assert_eq!(actual, expected);
    }
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

#[test]
fn rejected_samples_preserve_all_successful_statistics() {
    for initial in [vec![], vec![BTreeMap::from([(1, 900), (2, 700)])]] {
        for (refresh_fails, reading_fails, capacities) in [
            (true, false, BTreeMap::from([(1, 0)])),
            (false, true, BTreeMap::from([(1, 0)])),
            (false, false, BTreeMap::new()),
        ] {
            let samples = Samples::new();
            for capacities in initial.clone() {
                samples.accept(capacities, 1);
            }
            let rejected = if reading_fails {
                Err(io::Error::other("measurement failed"))
            } else {
                Ok(reading(capacities, 99))
            };
            let before = samples.snapshot();

            samples.sample(rejected, refresh_fails);
            let after = samples.snapshot();

            assert_eq!(after, before);
        }
    }
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
