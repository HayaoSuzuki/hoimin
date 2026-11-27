use super::ExceptionProject;
use camino::Utf8PathBuf;
use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Built,
    Cached,
    RawLimit,
    Fingerprint,
    Encoding,
    DecodedFileLimit,
    DecodedTotalLimit,
    AnalysisDepth,
    Cancelled,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Observation {
    kind: Kind,
    path: Option<Utf8PathBuf>,
    cached: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fingerprint {
    Matching,
    Changed,
    Missing,
}

#[derive(serde::Deserialize)]
struct Contents {
    path: Utf8PathBuf,
    bytes: Vec<u8>,
}

#[derive(serde::Deserialize)]
struct File {
    #[serde(flatten)]
    contents: Contents,
    fingerprint: Fingerprint,
}

#[derive(serde::Deserialize)]
struct Attempt {
    replacement: Option<Contents>,
    cancelled: bool,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    per_file: usize,
    total: usize,
    prepared: bool,
    files: Vec<File>,
    attempts: Vec<Attempt>,
    expected: Vec<Observation>,
}

fn observe_failure(error: &str, id: &str) -> (Kind, Option<Utf8PathBuf>) {
    match error {
        "analyzer.cancelled" => return (Kind::Cancelled, None),
        "exception hierarchy total decoded source byte limit exceeded" => {
            return (Kind::DecodedTotalLimit, None);
        }
        _ if error.starts_with("analysis depth exceeds supported limit ") => {
            return (Kind::AnalysisDepth, None);
        }
        _ => {}
    }
    let (path, detail) = error
        .split_once(": ")
        .unwrap_or_else(|| panic!("{id}: unexpected acquisition error: {error}"));
    let kind = if detail.starts_with("read hierarchy input failed for ")
        && detail.ends_with(": exception hierarchy source byte limit exceeded")
    {
        Kind::RawLimit
    } else if detail == "exception hierarchy input changed after fingerprint preparation" {
        Kind::Fingerprint
    } else if detail.starts_with("source encoding ") {
        Kind::Encoding
    } else if detail == "decoded source byte limit exceeded" {
        Kind::DecodedFileLimit
    } else {
        panic!("{id}: unexpected acquisition error: {error}");
    };
    (kind, Some(path.into()))
}

#[test]
fn exception_source_acquisition_matches_lean() {
    let corpus =
        include_str!("../../../../../formal/HoiminOracle/corpus/exception-acquisition.jsonl");
    let mut ids = BTreeSet::new();
    let mut observations = 0;
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert_eq!(case.attempts.len(), case.expected.len(), "{}", case.id);
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();
        for file in &case.files {
            fs::write(root.join(&file.contents.path), &file.contents.bytes).unwrap();
        }
        let mut project = ExceptionProject::new(&root);
        if case.prepared {
            let inputs = case
                .files
                .into_iter()
                .filter_map(|file| {
                    let digest = match file.fingerprint {
                        Fingerprint::Matching => blake3::hash(&file.contents.bytes),
                        Fingerprint::Changed => blake3::hash(b"different prepared bytes"),
                        Fingerprint::Missing => return None,
                    };
                    Some((file.contents.path, digest.to_hex().to_string()))
                })
                .collect();
            project.prepared_inputs = Some(Arc::new(inputs));
        }

        for (index, (attempt, expected)) in case.attempts.into_iter().zip(case.expected).enumerate()
        {
            observations += 1;
            if let Some(contents) = attempt.replacement {
                fs::write(root.join(contents.path), contents.bytes).unwrap();
            }
            let previous = project.index.lock().unwrap().clone();
            let result =
                project.load_with_byte_limits(case.per_file, case.total, &|| attempt.cancelled);
            let current = project.index.lock().unwrap();
            if let Some(previous) = &previous {
                assert!(
                    current
                        .as_ref()
                        .is_some_and(|now| Arc::ptr_eq(previous, now)),
                    "{} attempt {index} replaced the cached index",
                    case.id
                );
            }
            let (kind, path) = match result {
                Ok(result) => {
                    assert!(
                        current
                            .as_ref()
                            .is_some_and(|now| Arc::ptr_eq(&result, now))
                    );
                    (
                        if previous.is_some() {
                            Kind::Cached
                        } else {
                            Kind::Built
                        },
                        None,
                    )
                }
                Err(error) => observe_failure(&error, &case.id),
            };
            let observed = Observation {
                kind,
                path,
                cached: current.is_some(),
            };
            assert_eq!(observed, expected, "{} attempt {index}", case.id);
        }
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 129);
    assert_eq!(observations, 134);
}
