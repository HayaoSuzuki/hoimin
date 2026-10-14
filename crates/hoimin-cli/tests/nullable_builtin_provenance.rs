#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use hoimin_cli::plan::PlanManifest;

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

async fn plan(source: &str) -> PlanManifest {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("subject.py"), source).unwrap();
    let args: Vec<OsString> = vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        directory.path().as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        "type_nullable_add".into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--".into(),
        python().into_os_string(),
        "-c".into(),
        "pass".into(),
    ];
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    serde_json::from_slice(&stdout.snapshot()).unwrap()
}

fn evaluate(source: &str, expression: &str) {
    let harness = "import sys\nassert sys.version_info[:2] == (3, 14), sys.version\nnamespace = {}\nexec(sys.argv[1], namespace)\nnamespace['__annotations__'] = namespace['__annotate__'](1)\nassert eval(sys.argv[2], namespace)\n";
    let result = Command::new(python())
        .args(["-c", harness, source, expression])
        .output()
        .expect("CPython 3.14 must be available in .venv");
    assert!(
        result.status.success(),
        "{source}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[tokio::test]
async fn nullable_builtin_provenance_public_plan_matches_executable_annotations() {
    let custom_class = "class Meta(type):\n    def __or__(cls, other):\n        raise RuntimeError('custom union')\nclass int(metaclass=Meta): pass\nvalue: int\n";
    evaluate(custom_class, "__annotations__['value'] is int");
    evaluate(
        &format!(
            "{custom_class}\ntry:\n    int | None\nexcept RuntimeError as error:\n    assert str(error) == 'custom union'\nelse:\n    raise AssertionError('custom union must raise')\n"
        ),
        "__annotations__['value'] is int",
    );
    assert_eq!(plan(custom_class).await.candidates, Vec::new());
    for (annotation, assignment) in [
        ("str", "str = 7"),
        ("int", "int = 7"),
        ("float", "float = 7"),
        ("bool", "bool = 7"),
        ("bytes", "bytes = 7"),
        ("list[int]", "list = {int: 7}"),
        ("set[int]", "set = {int: 7}"),
        ("dict[str, int]", "dict = {(str, int): 7}"),
    ] {
        for source in [
            format!("{assignment}\nvalue: {annotation}\n"),
            format!("value: {annotation}\n{assignment}\n"),
        ] {
            evaluate(&source, "__annotations__['value'] == 7");
            assert!(plan(&source).await.candidates.is_empty(), "{source}");
        }
        let source = format!("value: {annotation}\n");
        evaluate(
            &source,
            &format!("__annotations__['value'] == {annotation}"),
        );
        let manifest = plan(&source).await;
        assert_eq!(manifest.candidates.len(), 1, "{source}");
        let candidate = &manifest.candidates[0];
        assert_eq!(candidate.original, annotation);
        assert_eq!(candidate.replacement, format!("{annotation} | None"));
        let start = usize::try_from(candidate.span.start).unwrap();
        let end = start + usize::try_from(candidate.span.length).unwrap();
        assert_eq!(&source[start..end], annotation);
        let mutated = format!(
            "{}{}{}",
            &source[..start],
            candidate.replacement,
            &source[end..]
        );
        evaluate(
            &mutated,
            &format!("__annotations__['value'] == ({annotation} | None)"),
        );
    }
}

#[tokio::test]
async fn nullable_builtin_provenance_public_plan_checks_unpacked_arguments() {
    for annotation in ["dict[*(str, int)]", "dict[*[str, int]]", "list[[int]]"] {
        let source = format!("int = 7\nvalue: {annotation}\n");
        evaluate(&source, "__annotations__['value'] is not None");
        assert!(plan(&source).await.candidates.is_empty(), "{source}");
        let source = format!("value: {annotation}\n");
        let manifest = plan(&source).await;
        assert_eq!(manifest.candidates.len(), 1, "{source}");
        assert_eq!(
            manifest.candidates[0].replacement,
            format!("{annotation} | None")
        );
        evaluate(
            &format!("value: {annotation} | None\n"),
            "__annotations__['value'] is not None",
        );
    }
}
