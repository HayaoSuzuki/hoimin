#![no_main]

#[path = "../../crates/hoimin-core/tests/support/budget_model.rs"]
mod model;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let commands = model::decode(data);
    model::check_history([2, u64::MAX, u64::MAX - 1], &commands);
});
