use hoimin_cli::cli::OutputFormat;
use hoimin_core::RunConfig;
use std::path::{Path, PathBuf};

fn config(root: &Path, extra: &[&str]) -> RunConfig {
    let mut args = vec![
        "hoimin",
        "run",
        "--root",
        root.to_str().unwrap(),
        "--file",
        "service.py",
        "--operators",
        "exception_hierarchy",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
    ];
    args.extend_from_slice(extra);
    args.extend(["--", "python3", "-c", "pass"]);
    hoimin_cli::cli::parse_config_from(args).unwrap()
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("errors.py"),
        "class Root(Exception): pass\nclass Child(Root): pass\nclass Sibling(Root): pass\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("service.py"),
        "from errors import Child, Root, Sibling\ndef f():\n    raise Child('x')\n",
    )
    .unwrap();
    dir
}

#[tokio::test]
async fn hierarchy_plan_verify_detects_dependency_changes_additions_and_deletions() {
    for change in ["modify", "add", "delete"] {
        let dir = fixture();
        let plan = hoimin_cli::plan::create(config(dir.path(), &[]))
            .await
            .unwrap();
        assert_eq!(plan.manifest.candidates.len(), 2);
        assert_eq!(plan.manifest.fingerprint_inputs.len(), 2);
        let ids = plan
            .manifest
            .candidates
            .iter()
            .map(|c| c.candidate.id.clone())
            .collect::<Vec<_>>();
        let saved = dir.path().join("plan.json");
        std::fs::write(&saved, serde_json::to_vec(&plan.manifest).unwrap()).unwrap();
        hoimin_cli::plan::prepare_verify(&saved, &ids, OutputFormat::Json)
            .await
            .unwrap();
        match change {
            "modify" => std::fs::write(dir.path().join("errors.py"), "class Root(Exception): pass\nclass Child(ValueError): pass\nclass Sibling(Root): pass\n").unwrap(),
            "add" => std::fs::write(dir.path().join("additional.py"), "class Additional(Exception): pass\n").unwrap(),
            _ => std::fs::remove_file(dir.path().join("errors.py")).unwrap(),
        }
        let error = hoimin_cli::plan::prepare_verify(&saved, &ids, OutputFormat::Json)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("fingerprint_input.changed"),
            "{change}: {error}"
        );
    }
}

#[tokio::test]
async fn hierarchy_exclusion_and_import_roots_apply_to_index_and_fingerprint() {
    let dir = fixture();
    let excluded = hoimin_cli::plan::create(config(dir.path(), &["--exclude", "errors.py"]))
        .await
        .unwrap();
    assert!(excluded.manifest.candidates.is_empty());
    assert_eq!(excluded.manifest.fingerprint_inputs.len(), 1);
    std::fs::create_dir(dir.path().join("lib")).unwrap();
    std::fs::rename(
        dir.path().join("errors.py"),
        dir.path().join("lib/errors.py"),
    )
    .unwrap();
    let included = hoimin_cli::plan::create(config(dir.path(), &["--import-root", "lib"]))
        .await
        .unwrap();
    assert_eq!(included.manifest.candidates.len(), 2);
    assert_eq!(included.manifest.fingerprint_inputs.len(), 2);
}

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.venv")
        .join(if cfg!(windows) {
            "Scripts/python.exe"
        } else {
            "bin/python"
        })
}

#[tokio::test]
async fn hierarchy_run_uses_same_candidates_and_mutants_change_python_behavior() {
    let dir = fixture();
    let plan = hoimin_cli::plan::create(config(dir.path(), &[]))
        .await
        .unwrap();
    let test = "import service\nfrom errors import Child\ntry:\n service.f()\nexcept Exception as e:\n assert type(e) is Child\nelse:\n raise AssertionError('no exception')\n";
    let report = run_mutants(dir.path(), test).await;
    assert_eq!(report["summary"]["counts"]["killed"], 2, "{report}");
    let mutants = report["mutants"].as_array().unwrap();
    assert_eq!(mutants.len(), plan.manifest.candidates.len());
    let report_text = report.to_string();
    for candidate in &plan.manifest.candidates {
        assert!(report_text.contains(&candidate.candidate.id));
    }
}

async fn run_mutants(root: &Path, test: &str) -> serde_json::Value {
    let executable = python();
    let args = [
        "hoimin",
        "run",
        "--root",
        root.to_str().unwrap(),
        "--file",
        "service.py",
        "--operators",
        "exception_hierarchy",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
        "--",
        executable.to_str().unwrap(),
        "-c",
        test,
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
    serde_json::from_slice(&stdout).unwrap()
}

#[tokio::test]
async fn hierarchy_rejects_duplicate_module_identities_and_shadowed_namespace_portions() {
    for duplicate in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("lib/pkg")).unwrap();
        std::fs::create_dir_all(dir.path().join("pkg")).unwrap();
        let classes = "class Root(Exception): pass\nclass Child(Root): pass\n";
        if duplicate {
            std::fs::write(dir.path().join("lib/errors.py"), classes).unwrap();
            std::fs::write(dir.path().join("service.py"),"from lib.errors import Root\nfrom errors import Child\ndef f():\n    raise Child()\n").unwrap();
        } else {
            std::fs::write(dir.path().join("pkg/errors.py"), classes).unwrap();
            std::fs::write(dir.path().join("lib/pkg/__init__.py"), "").unwrap();
            std::fs::write(
                dir.path().join("lib/pkg/errors.py"),
                "class Root(Exception): pass\nclass Child(ValueError): pass\n",
            )
            .unwrap();
            std::fs::write(
                dir.path().join("service.py"),
                "from pkg.errors import Root, Child\ndef f():\n    raise Child()\n",
            )
            .unwrap();
        }
        let plan = hoimin_cli::plan::create(config(dir.path(), &["--import-root", "lib"]))
            .await
            .unwrap();
        assert!(plan.manifest.candidates.is_empty(), "duplicate={duplicate}");
    }
}

#[tokio::test]
async fn hierarchy_handler_narrowing_changes_python_except_and_except_star_behavior() {
    for handler in ["except", "except*"] {
        let dir = fixture();
        let source = format!(
            "from errors import Root, Child, Sibling\ndef f(action):\n    caught = False\n    try:\n        action()\n    {handler} Root:\n        caught = True\n    return caught\n"
        );
        std::fs::write(dir.path().join("service.py"), source).unwrap();
        let test = "from errors import Child, Sibling\nfrom service import f\ndef child():\n raise Child('child')\ndef sibling():\n raise Sibling('sibling')\nassert f(child)\nassert f(sibling)\n";
        let report = run_mutants(dir.path(), test).await;
        assert_eq!(
            report["summary"]["counts"]["killed"], 2,
            "{handler}: {report}"
        );
        assert_eq!(report["mutants"].as_array().unwrap().len(), 2);
    }
}

fn write_sources(root: &Path, sources: &[(&str, &str)]) {
    for (path, source) in sources {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
}

#[tokio::test]
async fn hierarchy_audit_submodule_must_be_loaded_before_use() {
    let dir = tempfile::tempdir().unwrap();
    write_sources(
        dir.path(),
        &[
            ("pkg/__init__.py", "class Root(Exception): pass\n"),
            (
                "pkg/errors.py",
                "from pkg import Root\nclass Child(Root): pass\n",
            ),
            (
                "service.py",
                "import pkg as p\nfrom pkg import Root\ndef f():\n    raise Root()\nimport pkg.errors as later\n",
            ),
        ],
    );
    let plan = hoimin_cli::plan::create(config(dir.path(), &[]))
        .await
        .unwrap();
    assert!(
        plan.manifest.candidates.is_empty(),
        "{:?}",
        plan.manifest.candidates
    );
}

#[tokio::test]
async fn hierarchy_audit_relative_imports_cannot_mix_namespace_identities() {
    let dir = tempfile::tempdir().unwrap();
    write_sources(
        dir.path(),
        &[
            ("pkg/base.py", "class Root(ValueError): pass\n"),
            ("lib/pkg/base.py", "class Root(Exception): pass\n"),
            (
                "lib/pkg/errors.py",
                "from .base import Root\nclass Child(Root): pass\n",
            ),
            (
                "service.py",
                "from pkg.errors import Child\nfrom lib.pkg.base import Root\ndef f():\n    raise Child()\n",
            ),
        ],
    );
    let output = std::process::Command::new(python()).current_dir(dir.path())
        .args(["-B", "-c", "import sys; sys.path.append('lib'); from service import Child, Root; assert not issubclass(Child, Root)"])
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = hoimin_cli::plan::create(config(dir.path(), &["--import-root", "lib"]))
        .await
        .unwrap();
    assert!(
        plan.manifest.candidates.is_empty(),
        "{:?}",
        plan.manifest.candidates
    );
}

#[tokio::test]
async fn hierarchy_audit_index_bounds_visible_alias_expansion() {
    use std::fmt::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let mut errors = "class Root(Exception): pass\n".to_owned();
    let mut service = String::new();
    for i in 0..260 {
        writeln!(errors, "class Child{i}(Root): pass").unwrap();
        writeln!(service, "import errors as e{i}").unwrap();
    }
    service.push_str("def f():\n    raise e0.Child0()\n");
    write_sources(
        dir.path(),
        &[("errors.py", &errors), ("service.py", &service)],
    );
    let result = hoimin_cli::plan::create(config(dir.path(), &[])).await;
    assert!(
        result.is_err(),
        "alias expansion must hit the bounded index limit"
    );
    assert!(result.unwrap_err().to_string().contains("limit"));
}

#[tokio::test]
async fn hierarchy_audit_relative_imports_use_the_imported_module_name() {
    let dir = tempfile::tempdir().unwrap();
    write_sources(
        dir.path(),
        &[
            ("lib/pkg/__init__.py", ""),
            ("lib/pkg/base.py", "class Root(Exception): pass\n"),
            (
                "lib/pkg/errors.py",
                "from .base import Root\nclass Child(Root): pass\n",
            ),
            (
                "service.py",
                "from pkg.errors import Child\nfrom pkg.base import Root\ndef f():\n    raise Child()\n",
            ),
        ],
    );
    let plan = hoimin_cli::plan::create(config(dir.path(), &["--import-root", "lib"]))
        .await
        .unwrap();
    assert_eq!(plan.manifest.candidates.len(), 1);
    assert_eq!(plan.manifest.candidates[0].candidate.replacement, "Root");
}

#[tokio::test]
async fn hierarchy_audit_interpreter_modules_cannot_resolve_to_project_files() {
    for name in ["sys", "os", "__main__"] {
        let dir = tempfile::tempdir().unwrap();
        write_sources(
            dir.path(),
            &[
                ("base.py", "class Root(Exception): pass\n"),
                (
                    &format!("{name}.py"),
                    "from base import Root\nclass Child(Root): pass\n",
                ),
                (
                    "service.py",
                    &format!(
                        "from base import Root\nimport {name} as e\ndef f():\n    raise Root()\n"
                    ),
                ),
            ],
        );
        let output = std::process::Command::new(python())
            .current_dir(dir.path())
            .args([
                "-B",
                "-c",
                &format!("import {name} as e; assert not hasattr(e, 'Child')"),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let plan = hoimin_cli::plan::create(config(dir.path(), &[]))
            .await
            .unwrap();
        assert!(
            plan.manifest.candidates.is_empty(),
            "{name}: {:?}",
            plan.manifest.candidates
        );
    }
}

#[tokio::test]
async fn hierarchy_audit_early_submodules_and_nested_stdlib_names_remain_eligible() {
    let dir = tempfile::tempdir().unwrap();
    write_sources(
        dir.path(),
        &[
            ("pkg/__init__.py", "class Root(Exception): pass\n"),
            (
                "pkg/sys.py",
                "from pkg import Root\nclass Child(Root): pass\n",
            ),
            (
                "service.py",
                "import pkg as p\nimport pkg.sys as loaded\nfrom pkg import Root\ndef f():\n    raise Root()\n",
            ),
        ],
    );
    let plan = hoimin_cli::plan::create(config(dir.path(), &[]))
        .await
        .unwrap();
    assert_eq!(plan.manifest.candidates.len(), 1);
    assert_eq!(
        plan.manifest.candidates[0].candidate.replacement,
        "loaded.Child"
    );
}
