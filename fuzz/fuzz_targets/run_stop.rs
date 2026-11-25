#![no_main]

#[path = "../../crates/hoimin-core/tests/support/stop_model.rs"]
#[allow(dead_code)] // ITF observations are covered by the model replay harness.
mod model;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let (entry, cause, commands) = model::decode(data);
    model::check_history(entry, cause, &commands);
});
