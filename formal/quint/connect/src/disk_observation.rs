#[path = "../../../../crates/hoimin-core/tests/support/disk_observation_model.rs"]
#[allow(dead_code)] // Raw-ITF command decoding is covered by the ordinary replay harness.
mod model;
use model::{Command as C, Snapshot};
use proptest::prelude::*;
use quint_connect::runner::{self, RunConfig};
use quint_connect::{Driver, Result, State, Step, switch};
use serde::Deserialize;

struct Bridge {
    inner: model::Driver,
    skip: bool,
}
impl Driver for Bridge {
    type State = Snapshot;
    fn config() -> quint_connect::Config {
        quint_connect::Config {
            state: &["DiskObservation::DiskObservationModel::s"],
            nondet: &["DiskObservation::DiskObservationModel::dispatch"],
        }
    }
    fn step(&mut self, step: &Step) -> Result {
        switch!(step {
            Init => { self.inner = model::Driver::default(); },
            Observe(owned:u64,free:u64,elapsed:u64) => {
                if self.skip { self.skip=false; } else { self.inner.apply(C::Observe(owned,free,elapsed)); }
            },
            Fail(message:u64) => { self.inner.apply(C::Fail(message)); },
            Dispatch => { self.inner.apply(C::Dispatch); },
            Idle => {}
        })
    }
}
impl State<Bridge> for Snapshot {
    fn from_driver(driver: &Bridge) -> Result<Self> {
        Ok(driver.inner.snapshot())
    }
    fn from_spec(value: itf::Value) -> Result<Self> {
        Ok(Self::from_spec(&serde_json::Value::deserialize(value)?))
    }
}
fn run(seed: u32, schedule: &str, skip: bool) -> Result {
    runner::run_test(
        Bridge {
            inner: model::Driver::default(),
            skip,
        },
        runner::Config {
            test_name: format!("disk observation seed={seed} schedule={schedule}"),
            gen_config: RunConfig {
                spec: format!("{}/../disk_observation.qnt", env!("CARGO_MANIFEST_DIR")),
                main: Some("DiskObservation".into()),
                init: None,
                step: Some(schedule.into()),
                max_samples: Some(4),
                max_steps: Some(6),
                seed: seed.to_string(),
            },
        },
    )
}
#[rstest::rstest]
#[case::random("step")]
#[case::priority("priorityStep")]
#[case::deduplicate("duplicateStep")]
#[case::failure("failureStep")]
fn live_disk_observation_matches_public_lifecycle(#[case] schedule: &str) {
    // Arrange.
    let seed = 1172;
    // Act.
    let result = run(seed, schedule, false);
    // Assert.
    assert!(result.is_ok(), "{result:?}");
}
#[test]
fn live_disk_state_comparison_detects_missing_observation() {
    // Arrange.
    let skip = true;
    // Act.
    let error = run(1172, "priorityStep", skip).unwrap_err();
    // Assert: tool errors do not satisfy the negative control.
    assert!(
        error.to_string().contains("State invariant failed"),
        "{error:#}"
    );
}
proptest! {
    #![proptest_config(ProptestConfig { cases:8, max_shrink_iters:16, ..ProptestConfig::default() })]
    #[test]
    fn generated_disk_seed_matches_real_evidence(seed in any::<u32>()) {
        // Arrange: one seed generates four bounded histories.
        // Act.
        let result=run(seed,"step",false);
        // Assert.
        prop_assert!(result.is_ok(),"{result:?}");
    }
}
