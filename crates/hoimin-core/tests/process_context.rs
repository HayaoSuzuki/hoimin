use hoimin_core::{
    BaselineContext, EffectId, MutantContext, ProbeContext, ProcessLimits, RunEffect, RunProcess,
};
use proptest::prelude::*;
use std::time::Duration;

#[test]
fn process_effect_rejects_obsolete_optional_metadata_shape() {
    // Arrange: the previous internal format allowed a baseline without a worker.
    let encoded = serde_json::json!({"RunBaseline": {
        "id": 7, "worker": null, "run_id": "run", "mutant_id": null,
        "argv": [], "cwd": ".", "limits": {
            "timeout": {"secs": 1, "nanos": 0}, "max_output_bytes": 1,
            "max_memory_bytes": 1, "max_processes": 1
        }
    }});

    // Act.
    let decoded = serde_json::from_value::<RunEffect>(encoded);

    // Assert.
    assert!(
        decoded.is_err(),
        "accepted obsolete independent optional metadata"
    );
}

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
    fn baseline_context_roundtrips_required_identity(worker in any::<u32>(), run_id in prop_oneof![Just(String::new()), any::<String>()]) {
        // Arrange.
        let effect = RunEffect::RunBaseline(process(BaselineContext { worker, run_id }));
        // Act.
        let encoded = serde_json::to_value(&effect).unwrap();
        let decoded = serde_json::from_value::<RunEffect>(encoded).unwrap();
        // Assert.
        prop_assert_eq!(decoded, effect);
    }

    #[test]
    fn mutant_context_roundtrips_required_identity(
        worker in any::<u32>(), run_id in prop_oneof![Just(String::new()), any::<String>()], mutant_id in prop_oneof![Just(String::new()), any::<String>()]
    ) {
        // Arrange.
        let effect = RunEffect::RunMutant(process(MutantContext { worker, run_id, mutant_id }));
        // Act.
        let encoded = serde_json::to_value(&effect).unwrap();
        let decoded = serde_json::from_value::<RunEffect>(encoded).unwrap();
        // Assert.
        prop_assert_eq!(decoded, effect);
    }

}

#[test]
fn baseline_rejects_missing_required_context_field() {
    for field in ["worker", "run_id"] {
        // Arrange.
        let effect = RunEffect::RunBaseline(process(BaselineContext {
            worker: 0,
            run_id: "run".into(),
        }));
        let mut encoded = serde_json::to_value(effect).unwrap();
        encoded["RunBaseline"]["context"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        // Act.
        let decoded = serde_json::from_value::<RunEffect>(encoded);
        // Assert.
        assert!(decoded.is_err(), "accepted missing {field}");
    }
}

#[test]
fn mutant_rejects_missing_required_context_field() {
    for field in ["worker", "run_id", "mutant_id"] {
        // Arrange.
        let effect = RunEffect::RunMutant(process(MutantContext {
            worker: 0,
            run_id: "run".into(),
            mutant_id: "mutant".into(),
        }));
        let mut encoded = serde_json::to_value(effect).unwrap();
        encoded["RunMutant"]["context"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        // Act.
        let decoded = serde_json::from_value::<RunEffect>(encoded);
        // Assert.
        assert!(decoded.is_err(), "accepted missing {field}");
    }
}

#[test]
fn baseline_rejects_mutant_context() {
    // Arrange.
    let context = MutantContext {
        worker: 0,
        run_id: "run".into(),
        mutant_id: "mutant".into(),
    };
    let encoded = serde_json::json!({"RunBaseline": process(context)});
    // Act.
    let decoded = serde_json::from_value::<RunEffect>(encoded);
    // Assert.
    assert!(decoded.is_err());
}

#[test]
fn workspace_effect_rejects_probe_context() {
    // Arrange.
    let encoded = serde_json::json!({"RunBaseline": process(ProbeContext)});
    // Act.
    let decoded = serde_json::from_value::<RunEffect>(encoded);
    // Assert.
    assert!(decoded.is_err());
}

#[test]
fn probe_roundtrips_without_workspace_identity() {
    // Arrange.
    let request = process(ProbeContext);
    // Act.
    let encoded = serde_json::to_value(&request).unwrap();
    let decoded = serde_json::from_value::<RunProcess<ProbeContext>>(encoded).unwrap();
    // Assert.
    assert_eq!(decoded, request);
}
