//! Native filesystem acceptance is tested on the platforms that admit these names.

use camino::Utf8Path;

use super::meter_entry_display;
#[cfg(any(target_os = "linux", windows))]
use super::{RootCapability, WalkState, measure_owned_tree_with_elapsed};

#[cfg(unix)]
#[test]
fn diagnostic_display_accepts_invalid_utf8_without_filesystem_access() {
    use std::os::unix::ffi::OsStrExt;

    let name = std::ffi::OsStr::from_bytes(b"generated\x80");

    let display = meter_entry_display(Utf8Path::new("owned"), name);

    assert_eq!(display, Utf8Path::new("owned/generated\u{fffd}"));
}

#[cfg(windows)]
#[test]
fn diagnostic_display_accepts_a_lone_surrogate_without_filesystem_access() {
    use std::os::windows::ffi::OsStringExt;

    let name = std::ffi::OsString::from_wide(&[u16::from(b'g'), 0xD800]);

    let display = meter_entry_display(Utf8Path::new("owned"), &name);

    assert_eq!(display.file_name(), Some("g\u{fffd}"));
}

#[test]
fn diagnostic_display_bounds_a_deep_path_at_a_character_boundary() {
    let parent = "é".repeat(2047);

    let display = meter_entry_display(Utf8Path::new(&parent), std::ffi::OsStr::new("éé"));

    assert!(display.as_str().len() <= 4096);
    assert!(display.as_str().starts_with(&parent));
}

#[cfg(target_os = "linux")]
#[test]
fn meter_counts_an_invalid_utf8_regular_file() {
    use std::os::unix::ffi::OsStrExt;

    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join(std::ffi::OsStr::from_bytes(b"f\x80")),
        b"12345",
    )
    .unwrap();
    let root = RootCapability::open(Utf8Path::from_path(temp.path()).unwrap()).unwrap();
    let mut state = WalkState::default();

    measure_owned_tree_with_elapsed(&root, &mut state, &|| std::time::Duration::ZERO).unwrap();

    assert_eq!(state.owned_bytes, 5);
    assert_eq!(state.entries, 1);
}

#[cfg(target_os = "linux")]
#[test]
fn meter_counts_files_in_nested_invalid_utf8_directories() {
    use std::os::unix::ffi::OsStrExt;

    let temp = tempfile::tempdir().unwrap();
    let nested = temp
        .path()
        .join(std::ffi::OsStr::from_bytes(b"d\x80"))
        .join(std::ffi::OsStr::from_bytes(b"n\x81"));
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        nested.join(std::ffi::OsStr::from_bytes(b"f\x82")),
        b"1234567",
    )
    .unwrap();
    let root = RootCapability::open(Utf8Path::from_path(temp.path()).unwrap()).unwrap();
    let mut state = WalkState::default();

    measure_owned_tree_with_elapsed(&root, &mut state, &|| std::time::Duration::ZERO).unwrap();

    assert_eq!(state.owned_bytes, 7);
    assert_eq!(state.entries, 3);
}

#[cfg(target_os = "linux")]
#[test]
fn meter_does_not_follow_an_invalid_utf8_symlink() {
    use std::os::unix::ffi::OsStrExt;

    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("large"), [0; 1024]).unwrap();
    std::os::unix::fs::symlink(
        outside.path(),
        temp.path().join(std::ffi::OsStr::from_bytes(b"l\x80")),
    )
    .unwrap();
    let root = RootCapability::open(Utf8Path::from_path(temp.path()).unwrap()).unwrap();
    let mut state = WalkState::default();

    measure_owned_tree_with_elapsed(&root, &mut state, &|| std::time::Duration::ZERO).unwrap();

    assert_eq!(state.owned_bytes, 0);
    assert_eq!(state.entries, 1);
}

#[cfg(windows)]
#[test]
fn meter_counts_an_accepted_lone_surrogate_file() {
    use std::os::windows::ffi::OsStringExt;

    let temp = tempfile::tempdir().unwrap();
    let name = std::ffi::OsString::from_wide(&[u16::from(b'f'), 0xD800]);
    match std::fs::write(temp.path().join(name), b"12345") {
        Ok(()) => (),
        Err(error) => {
            assert!(
                matches!(
                    error.kind(),
                    std::io::ErrorKind::InvalidInput | std::io::ErrorKind::PermissionDenied
                ) || error.raw_os_error() == Some(123),
                "unexpected lone-surrogate setup failure: {error}"
            );
            eprintln!("SKIP native lone-surrogate file: filesystem rejected name: {error}");
            return;
        }
    }
    let root = RootCapability::open(Utf8Path::from_path(temp.path()).unwrap()).unwrap();
    let mut state = WalkState::default();

    measure_owned_tree_with_elapsed(&root, &mut state, &|| std::time::Duration::ZERO).unwrap();

    assert_eq!(state.owned_bytes, 5);
    assert_eq!(state.entries, 1);
}
