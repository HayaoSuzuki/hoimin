use std::collections::BTreeSet;

use camino::Utf8PathBuf;

use super::{FingerprintInput, SourceHash, fingerprint, fingerprint_preimage};
use crate::{
    CommandArg, EnvironmentSnapshot, FingerprintInputFile, MutationProfile, ResourceMode,
    RunLimits, TargetSlice,
};

#[derive(serde::Deserialize)]
struct Input {
    sources: Vec<EncodedSource>,
    fingerprint_inputs: Vec<FingerprintInputFile>,
    environment: EnvironmentSnapshot,
    targets: Vec<TargetSlice>,
    import_roots: Vec<Utf8PathBuf>,
    source_roots: Vec<Utf8PathBuf>,
    includes: Vec<String>,
    excludes: Vec<String>,
    operators: Vec<String>,
    profile: MutationProfile,
    test_argv: Vec<CommandArg>,
    limits: RunLimits,
    resource_mode: ResourceMode,
}

#[derive(serde::Deserialize)]
struct EncodedSource {
    path: Utf8PathBuf,
    hash: [u8; 32],
}

impl From<Input> for FingerprintInput {
    fn from(value: Input) -> Self {
        Self {
            sources: value
                .sources
                .into_iter()
                .map(|entry| SourceHash {
                    path: entry.path,
                    hash: entry.hash,
                })
                .collect(),
            fingerprint_inputs: value.fingerprint_inputs,
            environment: value.environment,
            targets: value.targets,
            import_roots: value.import_roots,
            source_roots: value.source_roots,
            includes: value.includes,
            excludes: value.excludes,
            operators: value.operators,
            profile: value.profile,
            test_argv: value.test_argv,
            limits: value.limits,
            resource_mode: value.resource_mode,
        }
    }
}

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    id: String,
    input: Input,
    preimage_hex: String,
}

fn from_hex(encoded: &str) -> Vec<u8> {
    let (pairs, remainder) = encoded.as_bytes().as_chunks::<2>();
    assert_eq!(remainder, b"");
    pairs
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn run_fingerprint_preimage_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/run-fingerprint.jsonl"
    ));
    let mut ids = BTreeSet::new();
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        let input = case.input.into();
        let expected = from_hex(&case.preimage_hex);
        assert_eq!(fingerprint_preimage(&input), expected, "{}", case.id);
        assert_eq!(
            fingerprint(&input).as_bytes(),
            blake3::hash(&expected).as_bytes(),
            "{}",
            case.id
        );
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 38);
}
