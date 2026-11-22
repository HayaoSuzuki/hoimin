//! Replace entries between enumeration and handle acquisition without timing races.

use std::cell::Cell;
use std::time::Duration;

use camino::Utf8Path;
use proptest::prelude::*;
use rstest::rstest;

use super::{MAX_TREE_DEPTH, RootCapability, WalkState, measure_owned_tree_with_hooks};

fn observe_file_replacement(
    depth: usize,
    old_size: usize,
    new_size: usize,
) -> std::io::Result<u64> {
    let temp = tempfile::tempdir().unwrap();
    let temp = Utf8Path::from_path(temp.path()).unwrap();
    let tree = temp.join("tree");
    let mut leaf = tree.clone();
    for _ in 0..depth {
        leaf.push("d");
    }
    std::fs::create_dir_all(&leaf).unwrap();
    let victim = leaf.join("cache.dat");
    let replacement = temp.join("replacement.dat");
    std::fs::write(&victim, vec![1; old_size]).unwrap();
    std::fs::write(&replacement, vec![2; new_size]).unwrap();
    let root = RootCapability::open(&tree).unwrap();
    let replaced = Cell::new(false);
    let mut state = WalkState::default();

    measure_owned_tree_with_hooks(
        &root,
        &mut state,
        &|| Duration::ZERO,
        &|display| {
            if display == victim && !replaced.replace(true) {
                std::fs::rename(&replacement, &victim).unwrap();
            }
        },
        &|_| {},
        &|_| {},
    )?;
    assert!(replaced.get());
    Ok(state.owned_bytes)
}

#[rstest]
#[case::root(0)]
#[case::deepest(MAX_TREE_DEPTH)]
fn atomic_regular_file_replacement_counts_the_opened_file(#[case] depth: usize) {
    // Arrange
    let new_size = 37;

    // Act
    let measured = observe_file_replacement(depth, 5, new_size);

    // Assert
    assert_eq!(measured.unwrap(), new_size as u64);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn replacement_size_does_not_depend_on_the_enumerated_file(
        old_size in 0_usize..512,
        new_size in 0_usize..512,
        depth in 0_usize..4,
    ) {
        // Arrange: generated sizes include empty files, growth and shrinkage.
        let expected = new_size as u64;

        // Act
        let measured = observe_file_replacement(depth, old_size, new_size);

        // Assert
        prop_assert_eq!(measured.unwrap(), expected);
    }
}

#[rstest]
#[case::directory_to_directory(true, true)]
#[case::directory_to_file(true, false)]
#[case::file_to_directory(false, true)]
fn entry_replacement_involving_a_directory_fails_closed(
    #[case] old_directory: bool,
    #[case] new_directory: bool,
) {
    // Arrange
    let temp = tempfile::tempdir().unwrap();
    let temp = Utf8Path::from_path(temp.path()).unwrap();
    let tree = temp.join("tree");
    std::fs::create_dir(&tree).unwrap();
    let victim = tree.join("victim");
    let replacement = temp.join("replacement");
    for (path, directory) in [(&victim, old_directory), (&replacement, new_directory)] {
        if directory {
            std::fs::create_dir(path).unwrap();
            std::fs::write(path.join("must-not-count"), b"outside data").unwrap();
        } else {
            std::fs::write(path, b"file").unwrap();
        }
    }
    let root = RootCapability::open(&tree).unwrap();
    let replaced = Cell::new(false);
    let mut state = WalkState::default();

    // Act
    let result = measure_owned_tree_with_hooks(
        &root,
        &mut state,
        &|| Duration::ZERO,
        &|display| {
            if display == victim && !replaced.replace(true) {
                std::fs::rename(&victim, temp.join("parked")).unwrap();
                std::fs::rename(&replacement, &victim).unwrap();
            }
        },
        &|_| {},
        &|_| {},
    );

    // Assert
    assert!(result.unwrap_err().to_string().contains("identity changed"));
    assert!(replaced.get());
    assert_eq!(state.owned_bytes, 0);
}

#[test]
fn directory_replacement_between_inspection_and_descent_fails_closed() {
    // Arrange
    let temp = tempfile::tempdir().unwrap();
    let temp = Utf8Path::from_path(temp.path()).unwrap();
    let tree = temp.join("tree");
    let victim = tree.join("victim");
    std::fs::create_dir_all(&victim).unwrap();
    let root = RootCapability::open(&tree).unwrap();
    let replaced = Cell::new(false);
    let mut state = WalkState::default();

    // Act
    let result = measure_owned_tree_with_hooks(
        &root,
        &mut state,
        &|| Duration::ZERO,
        &|_| {},
        &|display| {
            if display == victim && !replaced.replace(true) {
                std::fs::rename(&victim, temp.join("parked")).unwrap();
                std::fs::create_dir(&victim).unwrap();
                std::fs::write(victim.join("must-not-count"), b"replacement data").unwrap();
            }
        },
        &|_| {},
    );

    // Assert
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("directory identity changed")
    );
    assert!(replaced.get());
    assert_eq!(state.owned_bytes, 0);
}
