//! Live Quint Connect checks are isolated from the shipping Rust workspace.
#[cfg(test)]
#[path = "../../../../crates/hoimin-core/tests/support/workers_model.rs"]
#[allow(dead_code)]
mod model;

#[cfg(test)]
mod tests {
    use super::model;
    use model::{Command as C, Snapshot};
    use proptest::prelude::*;
    use quint_connect::runner::{self, RunConfig};
    use quint_connect::{Driver, Result, State, Step, switch};
    use serde::Deserialize;

    struct Bridge<const SESSION: bool> {
        inner: Option<model::Driver>,
        skip_apply: bool,
    }
    impl<const SESSION: bool> Driver for Bridge<SESSION> {
        type State = Snapshot;
        fn config() -> quint_connect::Config {
            quint_connect::Config {
                state: if SESSION {
                    &["WorkersSession::WorkersModel::s"]
                } else {
                    &["Workers::WorkersModel::s"]
                },
                nondet: if SESSION {
                    &["WorkersSession::WorkersModel::dispatch"]
                } else {
                    &["Workers::WorkersModel::dispatch"]
                },
            }
        }
        fn step(&mut self, step: &Step) -> Result {
            switch!(step {
                Init => {self.inner=Some(model::Driver::new(SESSION,2));},
                Ack(worker:u8,outcome:u8) => {
                    let inner=self.inner.as_mut().unwrap();
                    if self.skip_apply && inner.snapshot().stages.get(usize::from(worker))==Some(&0) {self.skip_apply=false;}
                    else {assert_eq!(inner.apply(C::Ack(worker,outcome==2)),model::Verdict::Accepted);}
                },
                Drain(worker:u8) => {assert_eq!(self.inner.as_mut().unwrap().apply(C::Ack(worker,false)),model::Verdict::Accepted);},
                Global => {assert_eq!(self.inner.as_mut().unwrap().apply(C::Global),model::Verdict::Accepted);},
                Stop(cause:u8) => {assert_eq!(self.inner.as_mut().unwrap().apply(C::Stop(cause,1024)),model::Verdict::Accepted);},
                Probe(kind:String,worker:u8) => {
                    let (command,expected)=match kind.as_str() {"wrong"=>(C::Wrong(worker),model::Verdict::Wrong),"worker"=>(C::Worker(worker),model::Verdict::Worker),"identity"=>(C::Identity(worker,true),model::Verdict::Identity),other=>panic!("unknown probe {other}")};
                    assert_eq!(self.inner.as_mut().unwrap().apply(command),expected);
                }
            })
        }
    }
    impl<const SESSION: bool> State<Bridge<SESSION>> for Snapshot {
        fn from_driver(driver: &Bridge<SESSION>) -> Result<Self> {
            Ok(driver.inner.as_ref().unwrap().snapshot())
        }
        fn from_spec(value: itf::Value) -> Result<Self> {
            let value = serde_json::Value::deserialize(value)?;
            Ok(Self::from_spec_record(&value))
        }
    }
    fn run_for<const SESSION: bool>(seed: u32, step: &str, skip_apply: bool) -> Result {
        runner::run_test(
            Bridge::<SESSION> {
                inner: None,
                skip_apply,
            },
            runner::Config {
                test_name: format!("workers session={SESSION} seed={seed} step={step}"),
                gen_config: RunConfig {
                    spec: format!("{}/../workers.qnt", env!("CARGO_MANIFEST_DIR")),
                    main: Some(if SESSION { "WorkersSession" } else { "Workers" }.into()),
                    init: None,
                    step: Some(step.into()),
                    max_samples: Some(4),
                    max_steps: Some(32),
                    seed: seed.to_string(),
                },
            },
        )
    }
    fn run(session: bool, seed: u32, step: &str, skip_apply: bool) -> Result {
        if session {
            run_for::<true>(seed, step, skip_apply)
        } else {
            run_for::<false>(seed, step, skip_apply)
        }
    }
    #[rstest::rstest]
    #[case::random("step")]
    #[case::reverse("reverseStep")]
    #[case::persist_stop("completedStopStep")]
    fn live_scheduled_traces_match_public_implementation(
        #[values(false, true)] session: bool,
        #[case] schedule: &str,
    ) {
        // Arrange.
        let seed = 1172;
        // Act.
        let result = run(session, seed, schedule, false);
        // Assert.
        assert!(result.is_ok(), "{result:?}");
    }
    #[test]
    fn live_state_projection_detects_a_skipped_apply_response() {
        // Arrange: deliberately skip the real API call required by the first Ack.
        let skip_apply = true;
        // Act.
        let error = run(true, 1172, "reverseStep", skip_apply).unwrap_err();
        // Assert: a generation/tool failure cannot satisfy this semantic negative control.
        assert!(
            error.to_string().contains("State invariant failed"),
            "{error:#}"
        );
    }
    proptest! {
        #![proptest_config(ProptestConfig { cases:8, max_shrink_iters:16, ..ProptestConfig::default() })]
        #[test]
        fn generated_quint_seed_replays_real_worker_interleavings(session in any::<bool>(),seed in any::<u32>()) {
            // Arrange: one seed drives four specification-generated histories.
            // Act.
            let result=run(session,seed,"step",false);
            // Assert.
            prop_assert!(result.is_ok(),"{result:?}");
        }
    }
}

#[cfg(test)]
mod disk_observation;

#[cfg(test)]
mod budget;
