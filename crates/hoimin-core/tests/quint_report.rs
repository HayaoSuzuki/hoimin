#[path = "support/report_model.rs"]
#[allow(dead_code)] // The independent journal and byte decoder are exercised by other harnesses.
mod model;
#[path = "support/quint.rs"]
#[allow(dead_code)] // Shared ITF reader; this adapter consumes only numeric fields.
mod quint;
use hoimin_core::ReportSequence;
use model::{Command, Kind};
use rstest::rstest;

fn replay(trace: &str) -> Result<(), String> {
    let states = quint::states(trace);
    assert!(states.len() > 1, "nonempty model oracle");
    let mut actual = ReportSequence::new();
    for state in states.iter().skip(1) {
        let kind = match state["op"].as_str().unwrap() {
            "run_start" => Kind::Start,
            "mutant_start" => Kind::MutantStart,
            "mutant_finish" => Kind::MutantFinish,
            "diagnostic" => Kind::Diagnostic,
            "run_finish" => Kind::Finish,
            other => return Err(format!("unknown model event {other}")),
        };
        let command = Command {
            kind,
            run: u8::try_from(quint::number(&state["eventRun"])).unwrap(),
            mutant: u8::try_from(quint::number(&state["mutant"])).unwrap(),
            identity: quint::number(&state["mutantSequence"]),
            sequence: quint::number(&state["sequence"]),
        };
        let got = model::apply(&mut actual, command);
        let expected = state["result"].as_str().unwrap();
        if model::code(&got) != expected {
            return Err(format!("{command:?}: {got:?}, expected {expected}"));
        }
        // No private fields are read. Future generated events observe the state.
    }
    Ok(())
}
#[rstest]
#[case::trace_0(include_str!("../../../formal/quint/traces/report_0.itf.json"))]
#[case::trace_1(include_str!("../../../formal/quint/traces/report_1.itf.json"))]
#[case::trace_2(include_str!("../../../formal/quint/traces/report_2.itf.json"))]
#[case::trace_3(include_str!("../../../formal/quint/traces/report_3.itf.json"))]
#[case::interleaved(include_str!("../../../formal/quint/traces/report_interleavedStep_0.itf.json"))]
fn generated_report_matches_public_validator(#[case] trace: &str) {
    // Arrange.
    let oracle = trace;
    // Act.
    let outcome = replay(oracle);
    // Assert.
    assert_eq!(outcome, Ok(()));
}
