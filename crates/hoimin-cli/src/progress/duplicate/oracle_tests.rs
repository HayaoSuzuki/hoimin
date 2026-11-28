#[path = "../../../tests/support/progress_duplicate_cases.rs"]
mod cases;

use super::{DuplicateInputs, DuplicateReason, InputDisposition};
use cases::{Duplicate, Mode, Reason, paths, read_cases};
use std::{fs, io, path::Path};

fn set_file(path: &Path, contents: Option<u8>) {
    if let Some(label) = contents {
        fs::write(path, [label]).unwrap();
    } else {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => panic!("cannot remove fixture {path:?}: {error}"),
        }
    }
}

#[test]
fn duplicate_tracker_matches_lean_with_collisions_and_changing_files() {
    let mut count = 0;
    for case in read_cases()
        .into_iter()
        .filter(|case| case.mode == Mode::InternalFixture)
    {
        count += 1;
        let directory = tempfile::tempdir().unwrap();
        let paths = paths(&directory, &case);
        for (path, contents) in paths.iter().zip(case.initial) {
            set_file(path, contents);
        }
        let mut tracker = DuplicateInputs::default();
        let mut history = Vec::new();
        for (index, (action, expected)) in case.actions.iter().zip(&case.expected).enumerate() {
            if let Some(edit) = &action.edit {
                set_file(&paths[edit.path], edit.bytes);
            }
            let path = &paths[action.path];
            let digest = blake3::hash(&[action.fingerprint]);

            let actual = tracker.observe(path, digest, &history);

            let actual = actual.map(|duplicate| Duplicate {
                previous: duplicate.previous,
                reason: match duplicate.reason {
                    DuplicateReason::SamePath => Reason::SamePath,
                    DuplicateReason::IdenticalBytes => Reason::IdenticalBytes,
                },
            });
            assert_eq!(&actual, expected, "{} input {index}", case.id);
            history.push(InputDisposition {
                source: path.clone(),
                reason: None,
                duplicate: None,
            });
        }
    }
    assert_eq!(count, 71);
}
