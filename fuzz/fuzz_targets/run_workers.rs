#![no_main]

#[path = "../../crates/hoimin-core/tests/support/workers_model.rs"]
#[allow(dead_code)] // Direct ITF and Quint Connect own projected-state checks.
mod model;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let (session, jobs, commands) = model::decode(data);
    model::check_history(session, jobs, &commands);
});
