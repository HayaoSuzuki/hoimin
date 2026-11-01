#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;

use hoimin_cli::plan::RankedPlanCandidate;
use hoimin_core::RunConfig;
use std::path::{Path, PathBuf};

fn config(root: &Path, extra: &[&str]) -> RunConfig<hoimin_core::EnvironmentRequest> {
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

#[rstest::rstest]
#[case::modify_1(0)]
#[case::add_2(1)]
#[case::delete_3(2)]
#[tokio::test]
async fn hierarchy_plan_verify_detects_dependency_changes_additions_and_deletions(
    #[case] scenario_index: usize,
) {
    type ChangeDependency = fn(&Path);
    let cases: [(&str, ChangeDependency); 3] = [
        ("modify", |root| {
            std::fs::write(root.join("errors.py"), "class Root(Exception): pass\nclass Child(ValueError): pass\nclass Sibling(Root): pass\n").unwrap();
        }),
        ("add", |root| {
            std::fs::write(
                root.join("additional.py"),
                "class Additional(Exception): pass\n",
            )
            .unwrap();
        }),
        ("delete", |root| {
            std::fs::remove_file(root.join("errors.py")).unwrap();
        }),
    ];

    let (change, change_dependency) = cases.into_iter().nth(scenario_index).unwrap();

    let dir = fixture();
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(plan.manifest.candidates.len(), 2);
    assert_eq!(plan.manifest.normalized_config.fingerprint_inputs.len(), 2);
    let ids = plan
        .manifest
        .candidates
        .iter()
        .map(|c| c.candidate.id.clone())
        .collect::<Vec<_>>();
    let saved = dir.path().join("plan.json");
    std::fs::write(&saved, serde_json::to_vec(&plan.manifest).unwrap()).unwrap();
    hoimin_cli::plan::prepare_verify(&saved, &ids)
        .await
        .unwrap();
    change_dependency(dir.path());

    let error = hoimin_cli::plan::prepare_verify(&saved, &ids)
        .await
        .unwrap_err();

    assert!(
        error.to_string().contains("fingerprint_input.changed"),
        "{change}: {error}"
    );
}

#[tokio::test]
async fn hierarchy_exclusions_apply_to_index_and_fingerprint() {
    let dir = fixture();
    let excluded = hoimin_cli::plan::create(
        config(dir.path(), &["--exclude", "errors.py"]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        excluded.manifest.candidates,
        Vec::<RankedPlanCandidate>::new()
    );
    assert_eq!(
        excluded.manifest.normalized_config.fingerprint_inputs.len(),
        1
    );
}

#[tokio::test]
async fn hierarchy_import_roots_apply_to_index_and_fingerprint() {
    let dir = fixture();
    std::fs::create_dir(dir.path().join("lib")).unwrap();
    std::fs::rename(
        dir.path().join("errors.py"),
        dir.path().join("lib/errors.py"),
    )
    .unwrap();
    let included = hoimin_cli::plan::create(
        config(dir.path(), &["--import-root", "lib"]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(included.manifest.candidates.len(), 2);
    assert_eq!(
        included.manifest.normalized_config.fingerprint_inputs.len(),
        2
    );
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
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
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
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let status = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;
    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    jsonl_report::from_slice(&stdout.snapshot()).unwrap()
}

#[rstest::rstest]
#[case::duplicate_1(0)]
#[case::shadowed_namespace_2(1)]
#[tokio::test]
async fn hierarchy_rejects_duplicate_module_identities_and_shadowed_namespace_portions(
    #[case] scenario_index: usize,
) {
    let classes = "class Root(Exception): pass\nclass Child(Root): pass\n";

    let (name, sources) = [
        (
            "duplicate",
            vec![
                ("lib/errors.py", classes),
                (
                    "service.py",
                    "from lib.errors import Root\nfrom errors import Child\ndef f():\n    raise Child()\n",
                ),
            ],
        ),
        (
            "shadowed-namespace",
            vec![
                ("pkg/errors.py", classes),
                ("lib/pkg/__init__.py", ""),
                (
                    "lib/pkg/errors.py",
                    "class Root(Exception): pass\nclass Child(ValueError): pass\n",
                ),
                (
                    "service.py",
                    "from pkg.errors import Root, Child\ndef f():\n    raise Child()\n",
                ),
            ],
        ),
    ].into_iter().nth(scenario_index).unwrap();

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("lib/pkg")).unwrap();
    std::fs::create_dir_all(dir.path().join("pkg")).unwrap();
    write_sources(dir.path(), &sources);
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &["--import-root", "lib"]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert!(plan.manifest.candidates.is_empty(), "{name}");
}

#[rstest::rstest]
#[case::except_1("except")]
#[case::except_2("except*")]
#[tokio::test]
async fn hierarchy_handler_narrowing_changes_python_except_and_except_star_behavior(
    #[case] handler: &str,
) {
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
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
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
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &["--import-root", "lib"]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
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
    let result = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await;
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
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &["--import-root", "lib"]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(plan.manifest.candidates.len(), 1);
    assert_eq!(plan.manifest.candidates[0].candidate.replacement, "Root");
}

#[rstest::rstest]
#[case::sys_1("sys")]
#[case::os_2("os")]
#[case::main_3("__main__")]
#[tokio::test]
async fn hierarchy_audit_interpreter_modules_cannot_resolve_to_project_files(#[case] name: &str) {
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
                &format!("from base import Root\nimport {name} as e\ndef f():\n    raise Root()\n"),
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
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert!(
        plan.manifest.candidates.is_empty(),
        "{name}: {:?}",
        plan.manifest.candidates
    );
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
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(plan.manifest.candidates.len(), 1);
    assert_eq!(
        plan.manifest.candidates[0].candidate.replacement,
        "loaded.Child"
    );
}

#[rstest::rstest]
#[case::absent_1("absent", "", "", "False", 1)]
#[case::aliased_2("aliased", "import pkg.Child as loaded\n", "", "True", 0)]
#[case::from_3("from", "from pkg.Child import marker\n", "", "True", 0)]
#[case::nested_import_4(
    "nested-import",
    "def load():\n    import pkg.Child as loaded\nload()\n",
    "",
    "True",
    0
)]
#[case::nested_from_5(
    "nested-from",
    "def load():\n    from pkg.Child import marker\nload()\n",
    "",
    "True",
    0
)]
#[case::relative_6(
    "relative",
    "import pkg.loader\n",
    "from .Child import marker\n",
    "True",
    0
)]
#[case::nested_relative_7(
    "nested-relative",
    "import pkg.loader\npkg.loader.load()\n",
    "def load():\n    from .Child import marker\n",
    "True",
    0
)]
#[tokio::test]
async fn hierarchy_review_submodule_loading_overwrites_package_class_attributes(
    #[case] name: &str,
    #[case] imports: &str,
    #[case] helper: &str,
    #[case] runtime_imported: &str,
    #[case] expected_candidates: usize,
) {
    let dir = tempfile::tempdir().unwrap();
    let service = format!("import pkg\n{imports}def f():\n    raise pkg.Root()\n");
    write_sources(
        dir.path(),
        &[
            (
                "pkg/__init__.py",
                "class Root(Exception): pass\nclass Child(Root): pass\n",
            ),
            ("pkg/Child.py", "marker = 1\n"),
            ("pkg/loader.py", helper),
            ("service.py", &service),
        ],
    );
    let runtime = format!(
        "import service, types; assert isinstance(service.pkg.Child, types.ModuleType) == {runtime_imported}"
    );

    let output = std::process::Command::new(python())
        .current_dir(dir.path())
        .args(["-B", "-c", &runtime])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        plan.manifest.candidates.len(),
        expected_candidates,
        "{name}: {:?}",
        plan.manifest.candidates
    );
}

#[tokio::test]
async fn hierarchy_review_deferred_self_import_is_not_an_initialization_cycle() {
    let dir = tempfile::tempdir().unwrap();
    write_sources(
        dir.path(),
        &[
            (
                "errors.py",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef deferred():\n    import errors as own\n    return own.Root\n",
            ),
            (
                "service.py",
                "from errors import Root, Child\ndef f():\n    raise Child()\n",
            ),
        ],
    );
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(plan.manifest.candidates.len(), 1);
    assert_eq!(plan.manifest.candidates[0].candidate.replacement, "Root");
}

#[tokio::test]
async fn hierarchy_review_bounds_deferred_import_dependencies() {
    use std::fmt::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let mut source = "def deferred():\n".to_owned();
    for i in 0..65_536 {
        writeln!(source, "    import dependency{i} as unused").unwrap();
    }
    std::fs::write(dir.path().join("service.py"), &source).unwrap();
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(plan.manifest.candidates, Vec::<RankedPlanCandidate>::new());
    source.push_str("    import one_more_dependency as unused\n");
    std::fs::write(dir.path().join("service.py"), source).unwrap();
    let error = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("summary limit"), "{error}");
}

#[rstest::rstest]
#[case::unchanged_1("unchanged", "import errors as e\n", "True", 1)]
#[case::assignment_2("assignment", "import errors as e\ne.Child = object\n", "False", 0)]
#[case::nested_3(
    "nested",
    "def patch():\n    import errors as e\n    e.Child = object\npatch()\n",
    "False",
    0
)]
#[case::setattr_4(
    "setattr",
    "import errors as e\nsetattr(e, 'Child', object)\n",
    "False",
    0
)]
#[tokio::test]
async fn hierarchy_review_imported_attribute_writes_invalidate_other_aliases(
    #[case] name: &str,
    #[case] patch: &str,
    #[case] runtime_subclass: &str,
    #[case] expected_candidates: usize,
) {
    let dir = tempfile::tempdir().unwrap();
    write_sources(
        dir.path(),
        &[
            (
                "errors.py",
                "class Root(Exception): pass\nclass Child(Root): pass\n",
            ),
            ("patcher.py", patch),
            (
                "service.py",
                "import patcher\nfrom errors import Root, Child\ndef f():\n    raise Root()\n",
            ),
        ],
    );
    let runtime = format!(
        "import service; assert issubclass(service.Child, BaseException) == {runtime_subclass}"
    );

    let output = std::process::Command::new(python())
        .current_dir(dir.path())
        .args(["-B", "-c", &runtime])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        plan.manifest.candidates.len(),
        expected_candidates,
        "{name}: {:?}",
        plan.manifest.candidates
    );
}

#[rstest::rstest]
#[case::builtins_1(
    "builtins",
    "import builtins as e\ne.Exception = object\n",
    "not issubclass(service.Root, BaseException)"
)]
#[case::unchanged_2(
    "unchanged",
    "import pkg.errors as e\n",
    "issubclass(service.e.Child, service.Root)"
)]
#[case::unrelated_3(
    "unrelated",
    "import pkg.other as e\ne.Child = object\n",
    "issubclass(service.e.Child, service.Root)"
)]
#[case::private_4(
    "private",
    "class P:\n    def patch(self):\n        import pkg.errors as __e\n        __e.Child = object\nP().patch()\n",
    "service.e.Child is object"
)]
#[case::delete_5(
    "delete",
    "import pkg.errors as e\ndel e.Child\n",
    "not hasattr(service.e, 'Child')"
)]
#[case::delattr_6(
    "delattr",
    "import pkg.errors as e\ndelattr(e, 'Child')\n",
    "not hasattr(service.e, 'Child')"
)]
#[case::relative_7(
    "relative",
    "from pkg import errors as unused\nfrom . import errors as e\ne.Child = object\n",
    "service.e.Child is object"
)]
#[case::constructor_8(
    "constructor",
    "from .errors import Child as e\ne.__init__ = lambda self, *args: None\n",
    "service.e.Child.__init__ is not service.Root.__init__"
)]
#[case::dotted_9(
    "dotted",
    "import pkg.errors\npkg.errors.Child = object\n",
    "service.e.Child is object"
)]
#[tokio::test]
async fn hierarchy_review_attribute_write_forms_match_python(
    #[case] name: &str,
    #[case] patch: &str,
    #[case] check: &str,
) {
    let dir = tempfile::tempdir().unwrap();
    write_sources(
        dir.path(),
        &[
            ("pkg/__init__.py", ""),
            (
                "pkg/errors.py",
                "class Root(Exception): pass\nclass Child(Root): pass\n",
            ),
            (
                "pkg/other.py",
                "class Root(Exception): pass\nclass Child(Root): pass\n",
            ),
            ("pkg/patcher.py", patch),
            (
                "service.py",
                "import pkg.patcher\nimport pkg.errors as e\nfrom pkg.errors import Root\ndef f():\n    raise Root()\n",
            ),
        ],
    );
    let runtime = format!(
        "import builtins\nsaved = builtins.Exception\ntry:\n    import service\n    assert {check}\nfinally:\n    builtins.Exception = saved\n"
    );

    let output = std::process::Command::new(python())
        .current_dir(dir.path())
        .args(["-B", "-c", &runtime])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    let eligible = matches!(name, "unchanged" | "unrelated");
    assert_eq!(
        plan.manifest.candidates.len(),
        usize::from(eligible),
        "{name}: {:?}",
        plan.manifest.candidates
    );
}

#[tokio::test]
async fn hierarchy_review_bounds_import_alias_correlations() {
    use std::fmt::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let mut source = "def deferred():\n".to_owned();
    for i in 0..65_536 {
        writeln!(source, "    import errors as alias{i}").unwrap();
    }
    std::fs::write(dir.path().join("service.py"), &source).unwrap();
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(plan.manifest.candidates, Vec::<RankedPlanCandidate>::new());
    source.push_str("    import errors as one_more_alias\n");
    std::fs::write(dir.path().join("service.py"), source).unwrap();
    let error = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("summary limit"), "{error}");
}

#[rstest::rstest]
#[case::service_to_owned_1(0)]
#[case::service_replace_raise_child_raise_root_2(1)]
#[tokio::test]
async fn hierarchy_assignment_alias_matches_runtime_rebinding(#[case] scenario_index: usize) {
    let dir = fixture();
    let service = "from errors import Root, Child\ndef f():\n    raise Child('x')\n";
    std::fs::write(dir.path().join("service.py"), service).unwrap();
    std::fs::write(
        dir.path().join("patcher.py"),
        "import errors as e\nother = e\nother.Root = object\n",
    )
    .unwrap();
    let runtime = "import patcher, service\ntry:\n service.f()\nexcept Exception as e:\n print(type(e).__name__)\n";

    let (source, expected) = [
        (service.to_owned(), "Child"),
        (service.replace("raise Child", "raise Root"), "TypeError"),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    std::fs::write(dir.path().join("service.py"), source).unwrap();

    let output = std::process::Command::new(python())
        .current_dir(dir.path())
        .args(["-B", "-c", runtime])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
}

#[tokio::test]
async fn hierarchy_assignment_alias_cannot_leave_rebound_destination_eligible() {
    let dir = fixture();
    let service = "from errors import Root, Child\ndef f():\n    raise Child('x')\n";
    std::fs::write(dir.path().join("service.py"), service).unwrap();
    std::fs::write(
        dir.path().join("patcher.py"),
        "import errors as e\nother = e\nother.Root = object\n",
    )
    .unwrap();

    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert!(
        plan.manifest.candidates.is_empty(),
        "{:?}",
        plan.manifest.candidates
    );
}

#[rstest::rstest]
#[case::input_1("", 1)]
#[case::other_init_lambda_self_code_none_2("other.__init__ = lambda self, code: None\n", 0)]
#[tokio::test]
async fn hierarchy_assignment_alias_invalidates_local_class_constructor(
    #[case] write: &str,
    #[case] expected: usize,
) {
    let dir = tempfile::tempdir().unwrap();

    let source = format!(
        "class Root(Exception): pass\nclass Child(Root): pass\nother = Root\n{write}def f():\n    raise Child()\n"
    );
    std::fs::write(dir.path().join("service.py"), source).unwrap();

    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(plan.manifest.candidates.len(), expected, "{write}");
}

#[tokio::test]
async fn hierarchy_assignment_edges_have_an_independent_limit() {
    use std::fmt::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let mut source = "def deferred():\n".to_owned();
    for i in 0..65_536 {
        writeln!(source, "    alias{i} = e").unwrap();
    }
    // Repeating an edge does not consume another retained entry.
    source.push_str("    alias0 = e\n");
    std::fs::write(dir.path().join("service.py"), &source).unwrap();
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(plan.manifest.candidates, Vec::<RankedPlanCandidate>::new());
    source.push_str("    one_more = e\n");
    std::fs::write(dir.path().join("service.py"), source).unwrap();
    let error = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("summary limit"), "{error}");
}

#[rstest::rstest]
#[case::disabled_1(0)]
#[case::tuple_2(1)]
#[case::no_relation_3(2)]
#[case::constructor_4(3)]
#[case::bare_5(4)]
#[tokio::test]
async fn hierarchy_outcomes_distinguish_skipped_analysis_from_policy_exclusions(
    #[case] scenario_index: usize,
) {
    let dir = tempfile::tempdir().unwrap();
    let definitions = "class Root(Exception): pass\nclass Child(Root): pass\n";

    let (name, source, reason) = [
        ("disabled", format!("{definitions}def target():\n    eval('1')\n    raise Child()\n"), Some("disabled_scope")),
        ("tuple", format!("{definitions}def target():\n    try: pass\n    except (Child, ValueError): pass\n"), Some("unsupported_expression")),
        ("no-relation", "class Child(Exception): pass\ndef target():\n    raise Child()\n".into(), None),
        ("constructor", "class Root(Exception):\n    def __init__(self, code): pass\nclass Child(Root): pass\ndef target():\n    raise Child(1)\n".into(), None),
        ("bare", format!("{definitions}def target():\n    raise\n"), None),
    ].into_iter().nth(scenario_index).unwrap();

    std::fs::write(dir.path().join("service.py"), source).unwrap();
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert!(plan.manifest.candidates.is_empty(), "{name}");
    assert_eq!(
        plan.manifest.diagnostics.len(),
        usize::from(reason.is_some()),
        "{name}: {:?}",
        plan.manifest.diagnostics
    );
    for (diagnostic, reason) in plan.manifest.diagnostics.iter().zip(reason) {
        assert!(diagnostic.message.contains(reason), "{diagnostic:?}");
        assert_eq!(diagnostic.line, Some(5), "{name}");
        assert!(diagnostic.column.is_some());
    }
    assert!(!plan.manifest.truncated);
}

#[tokio::test]
async fn hierarchy_skipped_reasons_are_aggregated_with_first_location() {
    let dir = tempfile::tempdir().unwrap();
    let source =
        "class Root(Exception): pass\nclass Child(Root): pass\ndef target():\n    eval('1')\n"
            .to_owned()
            + &"    raise Child()\n".repeat(200);
    std::fs::write(dir.path().join("service.py"), source).unwrap();
    let plan = hoimin_cli::plan::create(
        config(dir.path(), &[]),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(plan.manifest.diagnostics.len(), 1);
    let diagnostic = &plan.manifest.diagnostics[0];
    assert!(
        diagnostic.message.contains("disabled_scope=200"),
        "{diagnostic:?}"
    );
    assert_eq!(diagnostic.line, Some(5));
    assert!(diagnostic.message.len() < 512);
    assert!(!plan.manifest.truncated);
}
