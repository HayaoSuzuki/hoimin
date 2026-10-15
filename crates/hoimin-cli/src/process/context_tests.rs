use super::ProcessRequest;
use hoimin_core::{
    BaselineContext, EffectId, MutantContext, ProbeContext, ProcessLimits, RunProcess,
};
use proptest::prelude::*;
use std::time::Duration;

fn process<C>(context: C) -> RunProcess<C> {
    RunProcess {
        id: EffectId(42),
        context,
        argv: vec![],
        cwd: ".".into(),
        limits: ProcessLimits {
            timeout: Duration::from_secs(1),
            max_output_bytes: 1,
            max_memory_bytes: 1,
            max_processes: 1,
        },
    }
}

proptest! {
    #[test]
    fn baseline_lowering_preserves_worker(worker in any::<u32>()) {
        // Arrange.
        let input = process(BaselineContext { worker, run_id: "run".into() });
        // Act.
        let request = ProcessRequest::from(input);
        // Assert.
        prop_assert_eq!(request.process.context.worker(), Some(worker));
    }

    #[test]
    fn mutant_lowering_preserves_worker(worker in any::<u32>()) {
        // Arrange.
        let input = process(MutantContext { worker, run_id: "run".into(), mutant_id: "mutant".into() });
        // Act.
        let request = ProcessRequest::from(input);
        // Assert.
        prop_assert_eq!(request.process.context.worker(), Some(worker));
    }

    #[test]
    fn mutant_lowering_preserves_explicit_id(mutant_id in any::<String>()) {
        // Arrange.
        let input = process(MutantContext { worker: 0, run_id: "run".into(), mutant_id: mutant_id.clone() });
        // Act.
        let request = ProcessRequest::from(input);
        // Assert.
        prop_assert_eq!(request.process.context.mutant_id(), Some(mutant_id.as_str()));
    }
}

#[test]
fn probe_lowering_has_no_worker() {
    // Arrange.
    let input = process(ProbeContext);
    // Act.
    let request = ProcessRequest::from(input);
    // Assert.
    assert_eq!(request.process.context.worker(), None);
}

#[test]
fn baseline_lowering_has_no_mutant() {
    // Arrange.
    let input = process(BaselineContext {
        worker: 0,
        run_id: "run".into(),
    });
    // Act.
    let request = ProcessRequest::from(input);
    // Assert.
    assert_eq!(request.process.context.mutant_id(), None);
}
