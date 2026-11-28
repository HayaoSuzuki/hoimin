use crate::{
    artifacts::{ArtifactReadError, READ_ONLY_ARTIFACT_LIMIT, bounded_read},
    source_file::{PortableFileReadError, PortableFileReader, SourceFileError},
};
use camino::Utf8Path;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    fs::{self, File},
    io::Seek,
    path::Path,
    rc::Rc,
};

type BeforeRead = Box<dyn FnOnce(&File)>;

thread_local! {
    static BEFORE_READ: RefCell<Option<BeforeRead>> = const { RefCell::new(None) };
}

pub(crate) fn after_metadata(file: &File) {
    if let Some(callback) = BEFORE_READ.with_borrow_mut(Option::take) {
        callback(file);
    }
}

struct ResetHook;

impl Drop for ResetHook {
    fn drop(&mut self) {
        BEFORE_READ.with_borrow_mut(Option::take);
    }
}

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    limit: u64,
    observed: u64,
    bytes: Vec<u8>,
    consumed: Option<u64>,
    result: Verdict,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Verdict {
    TooLargeAtOpen,
    TooLargeWhenRead,
    Accepted { bytes: Vec<u8> },
}

fn observe<T>(case: &Case, path: &Path, read: impl FnOnce() -> T) -> (T, Option<u64>) {
    File::create(path).unwrap().set_len(case.observed).unwrap();
    let position = Rc::new(RefCell::new(None));
    let descriptor = Rc::clone(&position);
    let path = path.to_owned();
    let bytes = case.bytes.clone();
    BEFORE_READ.with_borrow_mut(|slot| {
        assert!(
            slot.replace(Box::new(move |file| {
                // The clone shares the cursor of the accepted descriptor.
                *descriptor.borrow_mut() = Some(file.try_clone().unwrap());
                fs::write(path, bytes).unwrap();
            }))
            .is_none()
        );
    });
    let reset = ResetHook;
    let result = read();
    drop(reset);
    let consumed = position
        .borrow_mut()
        .take()
        .map(|mut file: File| file.stream_position().unwrap());
    (result, consumed)
}

fn check_artifact(case: &Case, path: &Path) {
    let (result, consumed) = observe(case, path, || bounded_read(path, case.limit));
    let actual = match result {
        Ok(bytes) => Verdict::Accepted { bytes },
        Err(ArtifactReadError::Invalid(detail))
            if detail
                == format!(
                    "input exceeds size limit or is not a file: {}",
                    path.display()
                ) =>
        {
            Verdict::TooLargeAtOpen
        }
        Err(ArtifactReadError::Invalid(detail)) if detail == "input exceeds size limit" => {
            Verdict::TooLargeWhenRead
        }
        Err(error) => panic!("{}: unexpected artifact error {error:?}", case.id),
    };
    assert_eq!(consumed, case.consumed, "{}: artifact cursor", case.id);
    assert_eq!(actual, case.result, "{}: artifact outcome", case.id);
}

fn check_source(case: &Case, root: &Path, path: &Path) {
    let project = PortableFileReader::open(Utf8Path::from_path(root).unwrap()).unwrap();
    let relative = Utf8Path::new("input.bin");
    let (result, consumed) = observe(case, path, || {
        project.read_bounded(relative, usize::try_from(case.limit).unwrap())
    });
    let actual = match result {
        Ok(bytes) => Verdict::Accepted { bytes },
        Err(PortableFileReadError::Other(SourceFileError::Io {
            operation: "read hierarchy input",
            path,
            message,
        })) if path == relative && message == "exception hierarchy source byte limit exceeded" => {
            if consumed.is_some() {
                Verdict::TooLargeWhenRead
            } else {
                Verdict::TooLargeAtOpen
            }
        }
        Err(error) => panic!("{}: unexpected source error {error:?}", case.id),
    };
    assert_eq!(consumed, case.consumed, "{}: source cursor", case.id);
    assert_eq!(actual, case.result, "{}: source outcome", case.id);
}

#[test]
fn bounded_artifact_and_source_reads_match_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/bounded-file-reads.jsonl"
    ));
    let mut ids = BTreeSet::new();
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        if case.id == "first-artifact-byte-over-real-limit" {
            assert_eq!(case.limit, READ_ONLY_ARTIFACT_LIMIT);
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input.bin");

        check_artifact(&case, &path);
        check_source(&case, directory.path(), &path);
    }
    assert_eq!(ids.len(), 257);
}
