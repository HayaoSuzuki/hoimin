use std::fs;

use camino::Utf8PathBuf;
use hoimin_core::Selection;

#[test]
fn discovers_python_files_and_reports_regular_files() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("pkg")).unwrap();
    fs::write(temp.path().join("pkg/a.py"), "x = 1\n").unwrap();
    fs::write(temp.path().join("pkg/data.txt"), "fixture\n").unwrap();

    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::from("pkg")],
        ..Selection::default()
    };
    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
    assert!(
        files
            .iter()
            .any(|file| file.path == "pkg/a.py" && file.is_python)
    );
    assert!(
        files
            .iter()
            .any(|file| file.path == "pkg/data.txt" && !file.is_python)
    );
}

#[test]
fn explicit_exclude_wins_over_include() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("pkg")).unwrap();
    fs::write(temp.path().join(".gitignore"), "pkg/generated.py\n").unwrap();
    fs::write(temp.path().join("pkg/a.py"), "x = 1\n").unwrap();
    fs::write(temp.path().join("pkg/generated.py"), "x = 2\n").unwrap();

    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::from("pkg")],
        includes: vec!["pkg/generated.py".into()],
        excludes: vec!["pkg/generated.py".into()],
        ..Selection::default()
    };
    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
    assert!(files.iter().any(|file| file.path == "pkg/a.py"));
    assert!(!files.iter().any(|file| file.path == "pkg/generated.py"));
}

#[test]
fn include_can_restore_a_gitignored_file() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("pkg")).unwrap();
    fs::write(temp.path().join(".gitignore"), "pkg/generated.py\n").unwrap();
    fs::write(temp.path().join("pkg/generated.py"), "x = 2\n").unwrap();

    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::from("pkg")],
        includes: vec!["pkg/generated.py".into()],
        ..Selection::default()
    };
    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
    assert!(files.iter().any(|file| file.path == "pkg/generated.py"));
}
