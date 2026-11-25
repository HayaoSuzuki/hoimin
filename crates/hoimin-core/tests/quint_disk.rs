#[path = "support/disk_model.rs"]
#[allow(dead_code)] // Command decoding and trajectory laws have separate PBT/fuzz harnesses.
mod model;
#[path = "support/quint.rs"]
mod quint;
use hoimin_core::disk::DiskLifecycle;
use model::Command as C;
use rstest::rstest;
fn replay(trace: &str) -> Result<(), String> {
    let states = quint::states(trace);
    assert!(states.len() > 1);
    let mut actual = DiskLifecycle::new(
        quint::set(&states[0]["owned"])
            .iter()
            .map(|r| model::root(u8::try_from(quint::number(r)).unwrap())),
    )
    .unwrap();
    for row in &states {
        let arg = u8::try_from(quint::number(&row["arg"])).unwrap();
        let root = u8::try_from(quint::number(&row["root"])).unwrap();
        let got = match row["op"].as_str().unwrap() {
            "init" => true,
            "dispatch" => actual.apply(model::event(C::Dispatch)),
            "stop" => actual.apply(model::event(C::Stop)),
            name @ ("process" | "output" | "monitor" | "report") => {
                actual.apply(model::event(C::Component(
                    match name {
                        "process" => 0,
                        "output" => 1,
                        "monitor" => 2,
                        _ => 3,
                    },
                    arg == 1,
                )))
            }
            "request" => actual.apply(model::event(C::Request(root))),
            "cleanup" => actual.apply(model::event(C::Cleanup(root, arg))),
            "finish" => actual.apply(model::event(C::Finish)),
            other => return Err(format!("unknown action {other}")),
        };
        let snapshot = actual.snapshot();
        let got_fields = [
            snapshot.active,
            snapshot.dispatched,
            model::component(snapshot.process_drain),
            model::component(snapshot.output_drain),
            model::component(snapshot.monitor_join),
            model::component(snapshot.report),
            model::cleanup(&snapshot, 0),
            model::cleanup(&snapshot, 1),
        ];
        let expected = [
            "active",
            "dispatched",
            "process",
            "output",
            "monitor",
            "report",
            "executionCleanup",
            "deliveryCleanup",
        ]
        .map(|field| quint::number(&row[field]));
        if got != row["accepted"].as_bool().unwrap()
            || snapshot.finished != row["finished"].as_bool().unwrap()
            || snapshot.stop.is_some() != row["stopped"].as_bool().unwrap()
            || got_fields != expected
        {
            return Err(format!("{row}: got {snapshot:?}, accepted {got}"));
        }
    }
    Ok(())
}
#[rstest]
#[case::disk_0(include_str!("../../../formal/quint/traces/disk_0.itf.json"))]
#[case::disk_1(include_str!("../../../formal/quint/traces/disk_1.itf.json"))]
#[case::disk_2(include_str!("../../../formal/quint/traces/disk_2.itf.json"))]
#[case::disk_3(include_str!("../../../formal/quint/traces/disk_3.itf.json"))]
#[case::disk_clean(include_str!("../../../formal/quint/traces/disk_cleanStep_0.itf.json"))]
#[case::disk_execution_0(include_str!("../../../formal/quint/traces/disk_execution_0.itf.json"))]
#[case::disk_execution_1(include_str!("../../../formal/quint/traces/disk_execution_1.itf.json"))]
#[case::disk_execution_2(include_str!("../../../formal/quint/traces/disk_execution_2.itf.json"))]
#[case::disk_execution_3(include_str!("../../../formal/quint/traces/disk_execution_3.itf.json"))]
#[case::disk_execution_clean(include_str!("../../../formal/quint/traces/disk_execution_cleanStep_0.itf.json"))]
#[case::disk_empty_0(include_str!("../../../formal/quint/traces/disk_empty_0.itf.json"))]
#[case::disk_empty_1(include_str!("../../../formal/quint/traces/disk_empty_1.itf.json"))]
#[case::disk_empty_2(include_str!("../../../formal/quint/traces/disk_empty_2.itf.json"))]
#[case::disk_empty_3(include_str!("../../../formal/quint/traces/disk_empty_3.itf.json"))]
#[case::disk_empty_clean(include_str!("../../../formal/quint/traces/disk_empty_cleanStep_0.itf.json"))]
#[case::delivery_deferred(include_str!("../../../formal/quint/traces/disk_deferredStep_0.itf.json"))]
fn model_matches_public_disk_lifecycle(#[case] trace: &str) {
    // Arrange.
    let oracle = trace;
    // Act.
    let outcome = replay(oracle);
    // Assert.
    assert_eq!(outcome, Ok(()));
}
