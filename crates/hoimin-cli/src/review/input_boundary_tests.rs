use camino::Utf8PathBuf;
#[cfg(unix)]
use hoimin_core::CandidateDescriptor;
use hoimin_core::{CandidateIdentity, MutationCandidate, stable_mutant_id};

use super::{BEFORE_SOURCE_OPEN, SOURCE_WORK, source_state, source_states};
use crate::review::model::SourceState;

fn fixture(path: &str) -> (tempfile::TempDir, Utf8PathBuf, MutationCandidate) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    std::fs::write(root.join(path), b"1\n").unwrap();
    let mut descriptor = crate::review::tests::key().descriptor;
    descriptor.path = path.into();
    let candidate = MutationCandidate {
        id: stable_mutant_id(&CandidateIdentity::from(&descriptor)).to_string(),
        sequence: 1,
        path: descriptor.path,
        span: descriptor.span,
        original: descriptor.original,
        replacement: descriptor.replacement,
        operator: descriptor.operator,
        line: descriptor.line,
        column: descriptor.column,
        symbol: descriptor.symbol,
        file_hash: descriptor.file_hash,
    };
    (dir, root, candidate)
}

#[cfg(unix)]
fn assert_rejected(result: Result<SourceState, super::ReviewError>) {
    assert_eq!(result.unwrap(), SourceState::Unavailable);
}

#[cfg(unix)]
#[test]
fn final_entry_swap_before_open_does_not_read_outside_source() {
    let (_dir, root, candidate) = fixture("sample.py");
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("sample.py"), b"1\n").unwrap();
    std::fs::write(root.join("sample.py"), b"9\n").unwrap();
    let target = root.join("sample.py");
    let outside_path = outside.path().join("sample.py");
    BEFORE_SOURCE_OPEN.with_borrow_mut(|hook| {
        *hook = Some(Box::new(move || {
            std::fs::remove_file(&target).unwrap();
            std::os::unix::fs::symlink(outside_path, target).unwrap();
        }));
    });
    assert_rejected(source_state(&root, &candidate));
}

#[cfg(unix)]
#[test]
fn intermediate_directory_swap_before_open_does_not_read_outside_source() {
    let (_dir, root, candidate) = fixture("package/sample.py");
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("sample.py"), b"1\n").unwrap();
    std::fs::write(root.join("package/sample.py"), b"9\n").unwrap();
    let target = root.join("package");
    let retained = root.join("retained");
    let outside_path = outside.path().to_owned();
    BEFORE_SOURCE_OPEN.with_borrow_mut(|hook| {
        *hook = Some(Box::new(move || {
            std::fs::rename(&target, retained).unwrap();
            std::os::unix::fs::symlink(outside_path, target).unwrap();
        }));
    });
    assert_rejected(source_state(&root, &candidate));
}

#[cfg(unix)]
#[test]
fn internal_final_and_directory_symlinks_are_rejected() {
    let (_dir, root, mut candidate) = fixture("real/sample.py");
    std::os::unix::fs::symlink(root.join("real/sample.py"), root.join("alias.py")).unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("alias")).unwrap();
    for path in ["alias.py", "alias/sample.py"] {
        candidate.path = path.into();
        candidate.id = stable_mutant_id(&CandidateIdentity::from(&CandidateDescriptor::from(
            &candidate,
        )))
        .to_string();
        assert_rejected(source_state(&root, &candidate));
    }
}

#[test]
fn exact_source_byte_limit_is_read_but_one_extra_byte_is_unavailable() {
    let (_dir, root, candidate) = fixture("sample.py");
    let file = std::fs::File::create(root.join("sample.py")).unwrap();
    file.set_len(crate::artifacts::READ_ONLY_ARTIFACT_LIMIT)
        .unwrap();
    SOURCE_WORK.with(|work| work.set((0, 0)));
    assert_eq!(source_state(&root, &candidate).unwrap(), SourceState::Stale);
    assert_eq!(SOURCE_WORK.with(std::cell::Cell::get), (1, 1));
    file.set_len(crate::artifacts::READ_ONLY_ARTIFACT_LIMIT + 1)
        .unwrap();
    assert_eq!(
        source_state(&root, &candidate).unwrap(),
        SourceState::Unavailable
    );
}

#[test]
fn duplicate_paths_share_the_observation_during_a_replacement() {
    let (_dir, root, candidate) = fixture("sample.py");
    let target = root.join("sample.py");
    BEFORE_SOURCE_OPEN.with_borrow_mut(|hook| {
        *hook = Some(Box::new(move || std::fs::write(target, b"9\n").unwrap()));
    });
    SOURCE_WORK.with(|work| work.set((0, 0)));
    let states = source_states(&root, &[&candidate, &candidate]);
    assert!(
        states
            .into_iter()
            .all(|state| matches!(state, Ok(SourceState::Stale)))
    );
    assert_eq!(SOURCE_WORK.with(std::cell::Cell::get), (1, 1));
}

#[cfg(unix)]
#[test]
fn fifo_source_is_unavailable_without_waiting_for_a_writer() {
    use std::os::unix::ffi::OsStrExt;
    use std::sync::mpsc;
    use std::time::Duration;

    let (_dir, root, candidate) = fixture("sample.py");
    let target = root.join("sample.py");
    std::fs::remove_file(&target).unwrap();
    let target = std::ffi::CString::new(target.as_os_str().as_bytes()).unwrap();
    // SAFETY: target is a valid NUL-terminated path and mode grants owner access.
    assert_eq!(unsafe { libc::mkfifo(target.as_ptr(), 0o600) }, 0);
    let (send, receive) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        send.send(source_state(&root, &candidate)).unwrap();
    });
    let state = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("FIFO read must not block");
    assert_eq!(state.unwrap(), SourceState::Unavailable);
    reader.join().unwrap();
}
