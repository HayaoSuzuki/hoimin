#![no_main]

#[path = "../../crates/hoimin-core/tests/support/disk_model.rs"]
mod model;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let (mask, bytes) = data
        .split_first()
        .map_or((3, &[][..]), |(mask, bytes)| (*mask & 3, bytes));
    let commands = model::decode(bytes);
    model::check_history(mask, &commands);
});
