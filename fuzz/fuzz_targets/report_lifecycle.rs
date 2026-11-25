#![no_main]
#[path = "../../crates/hoimin-core/tests/support/report_model.rs"]
mod model;
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let commands = model::decode(data);
    model::check_history(&commands);
});
