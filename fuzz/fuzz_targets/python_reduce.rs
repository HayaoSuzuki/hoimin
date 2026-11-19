#![no_main]
use hoimin_python::reduce::DefinitionSource;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if input.len() > 4096 { return; }
    let Some((&choice, bytes)) = input.split_first() else { return; };
    let Ok(source) = DefinitionSource::parse(bytes) else { return; };
    let Some(index) = usize::from(choice).checked_rem(source.definitions().len()) else { return; };
    let edited = source.remove(&[index], &[]).expect("whole-definition removal preserves syntax");
    assert!(DefinitionSource::parse(&edited).is_ok());
});
