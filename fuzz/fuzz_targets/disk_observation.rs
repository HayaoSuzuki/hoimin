#![no_main]
#[path = "../../crates/hoimin-core/tests/support/disk_observation_model.rs"]
#[allow(dead_code)] // Raw-ITF projections are used by the replay and Connect harnesses.
mod model;
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    let mut driver = model::Driver::default();
    for command in model::decode(bytes) {
        driver.apply(command);
    }
});
