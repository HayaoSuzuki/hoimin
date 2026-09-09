use camino::Utf8PathBuf;
use hoimin_cli::fingerprint_inputs::{FingerprintInputError, resolve};

struct FixtureRoot {
    _directory: tempfile::TempDir,
    root: Utf8PathBuf,
}

fn fixture_root(files: &[(&str, &str)]) -> FixtureRoot {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();
    for (path, contents) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    FixtureRoot {
        _directory: directory,
        root,
    }
}

fn assert_error_prefix(error: &FingerprintInputError, prefix: &str) {
    assert!(
        error.to_string().starts_with(prefix),
        "expected `{prefix}` prefix, got `{error}`"
    );
}

#[test]
fn resolve_sorts_deduplicates_and_hashes_matching_regular_files() {
    let fixture = fixture_root(&[("pyproject.toml", "a"), ("fixtures/b.json", "b")]);

    let records = resolve(
        &fixture.root,
        &[
            "fixtures/*.json".into(),
            "**/*.toml".into(),
            "fixtures/*.json".into(),
        ],
        &[],
    )
    .unwrap();

    assert_eq!(
        records
            .iter()
            .map(|record| record.path.as_str())
            .collect::<Vec<_>>(),
        ["fixtures/b.json", "pyproject.toml"]
    );
    assert_eq!(records[0].hash, blake3::hash(b"b").to_hex().to_string());
    assert_eq!(records[1].hash, blake3::hash(b"a").to_hex().to_string());
}

#[test]
fn resolve_considers_explicitly_named_ignored_files() {
    let fixture = fixture_root(&[(".gitignore", "ignored.json\n"), ("ignored.json", "data")]);

    let records = resolve(&fixture.root, &["ignored.json".into()], &[]).unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].path, "ignored.json");
}

#[test]
fn resolve_rejects_unmatched_patterns() {
    let fixture = fixture_root(&[("file.txt", "x")]);

    let error = resolve(&fixture.root, &["missing/*.json".into()], &[]).unwrap_err();

    assert_error_prefix(&error, "fingerprint.include.unmatched");
}

#[test]
fn resolve_rejects_absolute_parent_nul_and_invalid_glob_patterns() {
    let fixture = fixture_root(&[("file.txt", "x")]);

    for pattern in [
        "/tmp/x",
        "C:\\tmp\\x",
        "nested/file.toml:stream",
        "../x",
        "nested\\..\\x",
        "file\0name",
        "[",
    ] {
        let error = resolve(&fixture.root, &[pattern.into()], &[]).unwrap_err();
        assert_error_prefix(&error, "fingerprint.include.invalid_glob");
    }
}

#[test]
fn exact_file_rejects_colons_in_every_component() {
    let fixture = fixture_root(&[]);

    for path in ["cache:metadata/file.toml", "nested/file.toml:stream"] {
        let error = resolve(&fixture.root, &[], &[path.into()]).unwrap_err();
        assert_error_prefix(&error, "fingerprint.file.invalid_path");
    }
}

#[test]
fn resolve_rejects_directories() {
    let fixture = fixture_root(&[("file.txt", "x")]);
    std::fs::create_dir(fixture.root.join("dir")).unwrap();

    let error = resolve(&fixture.root, &["dir".into()], &[]).unwrap_err();

    assert_error_prefix(&error, "fingerprint.include.unsupported_file");
}

#[cfg(unix)]
#[test]
fn resolve_rejects_symlinks() {
    let fixture = fixture_root(&[("file.txt", "x")]);
    std::os::unix::fs::symlink("file.txt", fixture.root.join("link.txt")).unwrap();

    let error = resolve(&fixture.root, &["link.txt".into()], &[]).unwrap_err();

    assert_error_prefix(&error, "fingerprint.include.unsupported_file");
}

#[cfg(target_os = "linux")]
#[test]
fn resolve_rejects_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt;

    let fixture = fixture_root(&[]);
    let name = std::ffi::OsString::from_vec(vec![b'f', 0x80]);
    std::fs::write(fixture.root.as_std_path().join(name), "x").unwrap();

    let error = resolve(&fixture.root, &["*".into()], &[]).unwrap_err();

    assert_error_prefix(&error, "fingerprint.include.unsupported_file");
}

#[cfg(unix)]
#[test]
fn resolve_rejects_read_failures() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture_root(&[("unreadable.txt", "x")]);
    let path = fixture.root.join("unreadable.txt");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

    let result = resolve(&fixture.root, &["unreadable.txt".into()], &[]);

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let error = result.unwrap_err();
    assert_error_prefix(&error, "fingerprint.include.unsupported_file");
}

#[test]
fn exact_file_selects_only_the_named_root_relative_file() {
    let fixture = fixture_root(&[
        ("pyproject.toml", "root"),
        (".worktrees/a/pyproject.toml", "nested"),
    ]);

    let records = resolve(&fixture.root, &[], &["pyproject.toml".into()]).unwrap();

    assert_eq!(
        records
            .iter()
            .map(|record| record.path.as_str())
            .collect::<Vec<_>>(),
        ["pyproject.toml"]
    );
    assert_eq!(records[0].hash, blake3::hash(b"root").to_hex().to_string());
}

#[test]
fn glob_and_exact_file_are_deduplicated() {
    let fixture = fixture_root(&[("pyproject.toml", "root")]);

    let records = resolve(
        &fixture.root,
        &["pyproject.toml".into()],
        &["./pyproject.toml".into(), "pyproject.toml".into()],
    )
    .unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].path, "pyproject.toml");
}

#[test]
fn exact_binary_files_keep_expected_hashes_with_glob_overlap_and_aliases() {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();
    let bytes = [0, 0xff, b'\n', b'x'];
    std::fs::write(root.join("data.bin"), bytes).unwrap();

    let records = resolve(
        &root,
        &["*.bin".into()],
        &["./data.bin".into(), "data.bin".into()],
    )
    .unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].path, "data.bin");
    assert_eq!(records[0].hash, blake3::hash(&bytes).to_hex().to_string());
}

#[test]
fn exact_file_selection_order_does_not_change_sorted_records() {
    let fixture = fixture_root(&[("z.bin", "z"), ("a.bin", "a")]);

    let forward = resolve(&fixture.root, &[], &["z.bin".into(), "a.bin".into()]).unwrap();
    let reverse = resolve(&fixture.root, &[], &["a.bin".into(), "z.bin".into()]).unwrap();

    let expected = vec![
        hoimin_core::FingerprintInputFile {
            path: "a.bin".into(),
            hash: blake3::hash(b"a").to_hex().to_string(),
        },
        hoimin_core::FingerprintInputFile {
            path: "z.bin".into(),
            hash: blake3::hash(b"z").to_hex().to_string(),
        },
    ];
    assert_eq!(forward, expected);
    assert_eq!(reverse, expected);
}

#[test]
fn repeated_resolve_observes_updated_exact_file_contents() {
    let fixture = fixture_root(&[("data.bin", "before")]);

    let before = resolve(&fixture.root, &[], &["data.bin".into()]).unwrap();
    std::fs::write(fixture.root.join("data.bin"), b"after").unwrap();
    let after = resolve(&fixture.root, &[], &["data.bin".into()]).unwrap();

    assert_eq!(before[0].hash, blake3::hash(b"before").to_hex().to_string());
    assert_eq!(after[0].hash, blake3::hash(b"after").to_hex().to_string());
    assert_ne!(before, after);
}

#[test]
fn exact_file_treats_glob_metacharacters_literally() {
    let fixture = fixture_root(&[("settings[prod].toml", "x")]);

    let records = resolve(&fixture.root, &[], &["settings[prod].toml".into()]).unwrap();

    assert_eq!(records[0].path, "settings[prod].toml");
}

#[test]
fn glob_pattern_can_escape_metacharacters() {
    let fixture = fixture_root(&[("settings[prod].toml", "x")]);

    let records = resolve(&fixture.root, &[r"settings\[prod\].toml".into()], &[]).unwrap();

    assert_eq!(records[0].path, "settings[prod].toml");
}

#[cfg(unix)]
#[test]
fn exact_file_rejects_a_literal_backslash_with_its_original_spelling() {
    let fixture = fixture_root(&[(r"literal\settings.toml", "x")]);

    let error = resolve(&fixture.root, &[], &[r"literal\settings.toml".into()]).unwrap_err();

    assert_eq!(
        error.to_string(),
        r"fingerprint.file.invalid_path: literal\settings.toml"
    );
}

#[cfg(unix)]
#[test]
fn glob_walk_rejects_a_literal_backslash_with_its_original_spelling() {
    let fixture = fixture_root(&[(r"literal\settings.toml", "x")]);

    let error = resolve(&fixture.root, &["*".into()], &[]).unwrap_err();

    assert_eq!(
        error.to_string(),
        r"fingerprint.include.unsupported_file: literal\settings.toml"
    );
}

#[test]
fn exact_file_rejects_unsafe_and_missing_paths() {
    let fixture = fixture_root(&[("file.txt", "x")]);
    for path in [
        "",
        "/tmp/x",
        "C:\\tmp\\x",
        "\\\\server\\share\\x",
        "../x",
        "nested\\..\\x",
        "file\0name",
    ] {
        let error = resolve(&fixture.root, &[], &[path.into()]).unwrap_err();
        assert_error_prefix(&error, "fingerprint.file.invalid_path");
    }

    let error = resolve(&fixture.root, &[], &["missing.toml".into()]).unwrap_err();
    assert_error_prefix(&error, "fingerprint.file.not_found");
}

#[test]
fn exact_file_rejects_directories() {
    let fixture = fixture_root(&[("file.txt", "x")]);
    std::fs::create_dir(fixture.root.join("dir")).unwrap();

    let error = resolve(&fixture.root, &[], &["dir".into()]).unwrap_err();

    assert_error_prefix(&error, "fingerprint.file.unsupported_file");
}

#[test]
fn exact_files_preserve_input_order_and_original_error_spelling() {
    let fixture = fixture_root(&[]);
    std::fs::create_dir(fixture.root.join("dir")).unwrap();

    let error = resolve(&fixture.root, &[], &["dir".into(), "../bad".into()]).unwrap_err();
    assert_error_prefix(&error, "fingerprint.file.unsupported_file: dir");

    let error = resolve(
        &fixture.root,
        &[],
        &["./first-missing".into(), "earlier-missing".into()],
    )
    .unwrap_err();
    assert_error_prefix(&error, "fingerprint.file.not_found: ./first-missing");
}

#[cfg(unix)]
#[test]
fn exact_file_rejects_symlinks() {
    let fixture = fixture_root(&[("file.txt", "x")]);
    std::os::unix::fs::symlink("file.txt", fixture.root.join("link.txt")).unwrap();

    let error = resolve(&fixture.root, &[], &["link.txt".into()]).unwrap_err();

    assert_error_prefix(&error, "fingerprint.file.unsupported_file");
}

#[cfg(unix)]
#[test]
fn exact_file_rejects_symlinked_parent() {
    let fixture = fixture_root(&[]);
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "outside").unwrap();
    std::os::unix::fs::symlink(outside.path(), fixture.root.join("linked")).unwrap();

    let error = resolve(&fixture.root, &[], &["linked/secret.txt".into()]).unwrap_err();

    assert_error_prefix(&error, "fingerprint.file.unsupported_file");
}

#[cfg(unix)]
#[test]
fn exact_missing_file_beneath_symlinked_parent_does_not_probe_outside() {
    let fixture = fixture_root(&[]);
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), fixture.root.join("linked")).unwrap();

    let error = resolve(&fixture.root, &[], &["linked/missing.txt".into()]).unwrap_err();

    assert_error_prefix(&error, "fingerprint.file.unsupported_file");
}
