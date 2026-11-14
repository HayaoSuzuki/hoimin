use super::*;
use ruff_python_parser::parse_module;
use std::fmt::Write as _;

#[test]
fn report_retains_earliest_skipped_range_and_counts_repeated_reasons() {
    let mut report = HierarchyReport::default();
    report.skip(
        SkipReason::UnsupportedExpression,
        TextRange::new(20.into(), 25.into()),
    );
    report.skip(
        SkipReason::UnsupportedExpression,
        TextRange::new(2.into(), 5.into()),
    );
    let (range, diagnostic) = report.diagnostic().unwrap();

    let actual = diagnostic.contains("unsupported_expression=2");

    assert_eq!(range, TextRange::new(2.into(), 5.into()));
    assert!(actual);
}

#[rstest::rstest]
fn lexical_bindings_include_patterns_handlers_and_comprehension_walruses(
    #[values(
        "package",
        "global_name",
        "outer_name",
        "kind",
        "error",
        "saved",
        "item",
        "rest",
        "head",
        "tail",
        "captured",
        "condition",
        "default",
        "root"
    )]
    name: &str,
) {
    let parsed = parse_module(
        "import package.child\nfrom module import *\nglobal global_name\nnonlocal outer_name\n\
         try:\n    action()\nexcept (kind := Exception) as error:\n    saved = error\n\
         match value:\n    case {'key': item, **rest}: pass\n    case [head, *tail]: pass\n\
         result = [(captured := item) for item in values if (condition := item)]\n\
         callback = lambda x=(default := 1): x\nroot.attribute = 1\n",
    )
    .unwrap();
    let mut bindings = Bindings::default();
    bindings.visit_body(&parsed.syntax().body);
    let contains_name = bindings.names.contains(name);

    let actual = !bindings.names.contains("x");

    assert!(contains_name, "missing {name}");
    assert!(bindings.dynamic);
    assert!(actual);
}

#[rstest::rstest]
#[case::function(0, &["decorator", "annotation", "default", "result"], "body_only")]
#[case::class(1, &["class_decorator", "base", "meta"], "class_body")]
fn definition_headers_bind_decorators_annotations_bases_and_keywords(
    #[case] statement: usize,
    #[case] expected: &[&str],
    #[case] excluded: &str,
) {
    let parsed = parse_module(
        "@(decorator := identity)\ndef function(arg: (annotation := int) = (default := 1)) -> (result := int):\n    body_only = 1\n\
         @(class_decorator := identity)\nclass Child((base := Parent), metaclass=(meta := type)):\n    class_body = 1\n",
    ).unwrap();

    let bindings = header_bindings(&parsed.syntax().body[statement]);

    let actual = expected.iter().all(|name| bindings.names.contains(*name));
    let actual_2 = !bindings.names.contains(excluded);

    assert!(actual);
    assert!(actual_2);
}

#[test]
fn body_binding_collection_records_definition_names() {
    let parsed = parse_module(
        "@(decorator := identity)\ndef function(arg: (annotation := int) = (default := 1)) -> (result := int):\n    body_only = 1\n\
         @(class_decorator := identity)\nclass Child((base := Parent), metaclass=(meta := type)):\n    class_body = 1\n",
    ).unwrap();

    let mut bindings = Bindings::default();
    bindings.visit_body(&parsed.syntax().body);

    let actual = bindings.names.contains("function");
    let actual_2 = bindings.names.contains("Child");

    assert!(actual);
    assert!(actual_2);
}

#[test]
fn nondefinition_statement_has_no_header_bindings() {
    let parsed = parse_module("pass\n").unwrap();

    let bindings = header_bindings(&parsed.syntax().body[0]);

    let actual = bindings.names.is_empty();

    assert!(actual);
}

#[rstest::rstest]
#[case("external", 0, Some("external"))]
#[case("child", 1, Some("pkg.child"))]
#[case("child", 3, None)]
fn relative_module_handles_absolute_and_above_root_imports(
    #[case] imported: &str,
    #[case] level: u32,
    #[case] expected: Option<&str>,
) {
    let actual = relative_module(Utf8Path::new("pkg/mod.py"), "pkg.mod", imported, level);

    let actual_ = actual.as_deref();

    assert_eq!(actual_, expected);
}

#[test]
fn hierarchy_collection_propagates_emission_failure() {
    let path = Utf8Path::new("sample.py");
    let source = "class Root(Exception): pass\nclass Child(Root): pass\nclass Sibling(Root): pass\nclass Container:\n    def method(self):\n        raise Child()\ndef f():\n    try:\n        raise Child()\n    except Root:\n        pass\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();
    let parsed = parse_module(source).unwrap();

    let result = index.collect(path, parsed.syntax(), 100, &|| false, |_, _| {
        Err(AnalysisError::HierarchySourceChanged)
    });

    let actual = matches!(result, Err(AnalysisError::HierarchySourceChanged));

    assert!(actual);
}

fn hierarchy_checkpoint_observation(
    select: impl FnOnce(usize) -> usize,
) -> (
    usize,
    usize,
    usize,
    usize,
    Result<HierarchyReport, AnalysisError>,
    usize,
) {
    let path = Utf8Path::new("sample.py");
    let source = "class Root(Exception): pass\nclass Child(Root): pass\nclass Sibling(Root): pass\nclass Container:\n    def method(self):\n        raise Child()\ndef f():\n    try:\n        raise Child()\n    except Root:\n        pass\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();
    let parsed = parse_module(source).unwrap();
    let total = std::cell::Cell::new(0);
    let mut baseline_emitted = 0;
    let baseline = index
        .collect(
            path,
            parsed.syntax(),
            100,
            &|| {
                total.set(total.get() + 1);
                false
            },
            |_, _| {
                baseline_emitted += 1;
                Ok(())
            },
        )
        .unwrap();
    let checkpoint = select(total.get());
    let polls = std::cell::Cell::new(0);
    let mut emitted = 0;
    let result = index.collect(
        path,
        parsed.syntax(),
        100,
        &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        },
        |_, _| {
            emitted += 1;
            Ok(())
        },
    );
    (
        checkpoint,
        total.get(),
        baseline.emitted,
        baseline_emitted,
        result,
        emitted,
    )
}

#[rstest::rstest]
fn hierarchy_cancellation_preserves_start_final_and_completion_boundaries(
    #[values(0, 1, 2)] boundary: usize,
) {
    let (checkpoint, total, baseline_count, baseline_emitted, result, emitted) =
        hierarchy_checkpoint_observation(|total| [0, total - 1, total][boundary]);

    let actual = result.is_err();
    let actual_2 = result
        .as_ref()
        .err()
        .is_none_or(|error| *error == AnalysisError::Cancelled);
    let actual_3 = result
        .as_ref()
        .ok()
        .is_none_or(|report| report.emitted == emitted && emitted > 0);
    let actual_4 = result.is_err();

    assert!(baseline_count > 0);
    assert_eq!(baseline_count, baseline_emitted);
    assert_eq!(actual, checkpoint < total);
    assert!(actual_2);
    assert!(actual_3);
    assert_eq!(actual_4, boundary < 2);
}

proptest::proptest! {
    #[test]
    fn hierarchy_collection_propagates_cancellation_at_each_checkpoint(checkpoint in 0usize..4096) {
        let (checkpoint, total, baseline_count, baseline_emitted, result, emitted) = hierarchy_checkpoint_observation(|_| checkpoint);

        let actual = result.is_err();
        let actual_2 = result.as_ref().err().is_none_or(|error| *error == AnalysisError::Cancelled);
        let actual_3 = result.as_ref().ok().is_none_or(|report| report.emitted == emitted && emitted > 0);

    assert!(baseline_count > 0);
    assert_eq!(baseline_count, baseline_emitted);
    assert_eq!(actual, checkpoint < total);
    assert!(actual_2);
    assert!(actual_3);
    }
}

#[rstest::rstest]
#[case("sample.py", "b.Exception", usize::MAX, Some("Exception"))]
#[case("sample.py", "E", usize::MAX, Some("Exception"))]
#[case("sample.py", "E", 0, None)]
#[case("sample.py", "E.member", usize::MAX, None)]
#[case("sample.py", "O", usize::MAX, None)]
#[case("sample.py", "Root", usize::MAX, None)]
#[case("missing.py", "Exception", usize::MAX, None)]
fn builtin_base_requires_visible_ordinary_builtin_import(
    #[case] query_path: &str,
    #[case] name: &str,
    #[case] before: usize,
    #[case] expected: Option<&str>,
) {
    let path = Utf8Path::new("sample.py");
    let source = "import builtins as b\nfrom builtins import Exception as E\nfrom builtins import object as O\nclass Root(E): pass\nAlias = Root\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();

    let actual = index.builtin_base(Utf8Path::new(query_path), name, before.min(source.len()));

    let actual_ = actual.as_deref();

    assert_eq!(actual_, expected);
}

#[test]
fn class_alias_does_not_resolve_as_a_declared_class() {
    let path = Utf8Path::new("sample.py");
    let source = "import builtins as b\nfrom builtins import Exception as E\nfrom builtins import object as O\nclass Root(E): pass\nAlias = Root\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();

    let actual = index.resolve_class(&(path.to_owned(), "Alias".into()));

    let actual_ = actual.is_none();

    assert!(actual_);
}

#[test]
fn dynamic_module_does_not_resolve_builtin_base() {
    let path = Utf8Path::new("sample.py");
    let source = "exec(code)\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();

    let actual = index.builtin_base(path, "Exception", source.len());

    assert_eq!(actual, None);
}

#[rstest::rstest]
#[case(true)]
#[case(false)]
fn raised_exception_eligibility_checks_parent_constructibility(#[case] constructible: bool) {
    let mut index = ExceptionIndex::from_sources(&[], &[], &|| false).unwrap();
    let parent = (Utf8PathBuf::from("sample.py"), "Parent".to_owned());
    index.classes.insert(
        parent.clone(),
        Class {
            parent: None,
            constructible,
        },
    );
    let child = Class {
        parent: Some(parent),
        constructible: true,
    };
    let eligibility = index.related_eligibility(&child, ExceptionSite::Raise, None);

    let actual = matches!(eligibility, RelatedEligibility::Eligible);

    assert_eq!(actual, constructible);
}

#[test]
fn sparse_visible_aliases_do_not_substitute_unrelated_exception_classes() {
    let path = Utf8Path::new("service.py");
    let source = "from errors import Child, Unrelated\nraise Child()\n";
    let sources = vec![
        (Utf8PathBuf::from("errors.py"), "class Root(Exception): pass\nclass Child(Root): pass\nclass Sibling(Root): pass\nclass OtherSibling(Root): pass\nclass Unrelated(Exception): pass\n".to_owned()),
        (path.to_owned(), source.to_owned()),
    ];
    let index =
        ExceptionIndex::from_sources(&sources, &[Utf8PathBuf::from(".")], &|| false).unwrap();
    let replacements = index.replacements(
        path,
        "Child",
        source.len(),
        ExceptionSite::Raise,
        &BTreeSet::new(),
        (100, &|| false),
    );

    assert_eq!(
        replacements,
        SiteOutcome::Ineligible(IneligibleReason::NoVisibleDestination)
    );
}

#[rstest::rstest]
#[case("factory().attribute = 1", Vec::<&str>::new())]
#[case("try:\n pass\nexcept:\n pass", Vec::<&str>::new())]
#[case("match value:\n case _:\n  pass", Vec::<&str>::new())]
#[case("match value:\n case [*_]:\n  pass", Vec::<&str>::new())]
#[case("match value:\n case {'key': captured}:\n  pass", vec!["captured"])]
fn lexical_binding_collection_handles_unnamed_or_computed_targets(
    #[case] source: &str,
    #[case]
    #[by_ref]
    expected: &[&str],
) {
    let parsed = parse_module(source).unwrap();
    let mut bindings = Bindings::default();
    bindings.visit_body(&parsed.syntax().body);

    let actual = bindings
        .names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case("package/module.py", "package.module", "sibling", 0, Some("sibling"))]
#[case(
    "package/module.py",
    "package.module",
    "sibling",
    1,
    Some("package.sibling")
)]
#[case(
    "package/__init__.py",
    "package",
    "sibling",
    1,
    Some("package.sibling")
)]
#[case("package/__init__.py", "package", "", 1, Some("package"))]
#[case("package/module.py", "package.module", "sibling", 2, None)]
#[case("module.py", "module", "sibling", 3, None)]
fn relative_import_resolution_respects_package_and_root_boundaries(
    #[case] path: &str,
    #[case] current: &str,
    #[case] imported: &str,
    #[case] level: u32,
    #[case] expected: Option<&str>,
) {
    let actual = relative_module(Utf8Path::new(path), current, imported, level);

    assert_eq!(actual.as_deref(), expected);
}

fn run_map(
    paths: &[Utf8PathBuf],
    roots: &[Utf8PathBuf],
    cancelled: &dyn Fn() -> bool,
) -> Result<BTreeMap<String, Utf8PathBuf>, AnalysisError> {
    map_modules(paths, roots, &[], &cancelled)
}

#[rstest::rstest]
#[case(vec!["src/pkg/module.py", "src/pkg/module.py"], vec!["src"], vec!["pkg.module"])]
#[case(vec!["src/pkg.py", "src/pkg/child.py"], vec!["src"], vec!["pkg"])]
#[case(vec!["src/pkg/__init__.py", "other/pkg/child.py"], vec!["src", "other"], vec!["pkg"])]
#[case(vec!["src/pkg/__init__.py", "src/pkg/child.py"], vec!["src"], vec!["pkg", "pkg.child"])]
#[case(vec!["src/pkg.py", "src/pkg/__init__.py"], vec!["src"], vec![])]
fn module_mapping_keeps_duplicate_identity_and_rejects_children_of_other_packages(
    #[case] paths: Vec<&str>,
    #[case] roots: Vec<&str>,
    #[case]
    #[by_ref]
    expected: &[&str],
) {
    let paths = paths.into_iter().map(Utf8PathBuf::from).collect::<Vec<_>>();
    let roots = roots.into_iter().map(Utf8PathBuf::from).collect::<Vec<_>>();
    let modules = run_map(&paths, &roots, &|| false).unwrap();

    let actual = modules.keys().map(String::as_str).collect::<Vec<_>>();

    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case(1, MAX_SUMMARY_ENTRIES + 1)]
#[case(2, MAX_SUMMARY_ENTRIES / 2 + 1)]
fn module_mapping_limits_both_individual_roots_and_total_claims(
    #[case] roots_count: usize,
    #[case] per_root: usize,
) {
    let roots = (0..roots_count)
        .map(|root| Utf8PathBuf::from(format!("root{root}")))
        .collect::<Vec<_>>();
    let paths = roots
        .iter()
        .enumerate()
        .flat_map(|(root, path)| {
            (0..per_root).map(move |index| path.join(format!("module{root}_{index}.py")))
        })
        .collect::<Vec<_>>();

    let actual = matches!(
        run_map(&paths, &roots, &|| false),
        Err(AnalysisError::HierarchyLimit)
    );

    assert!(actual);
}

#[rstest::rstest]
fn module_mapping_honors_cancellation_during_path_collection(#[values(0, 1)] checkpoint: usize) {
    let paths = ["src/first.py", "src/second.py"].map(Utf8PathBuf::from);
    let roots = [Utf8PathBuf::from("src")];
    let polls = std::cell::Cell::new(0);

    let actual = matches!(
        run_map(&paths, &roots, &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        }),
        Err(AnalysisError::Cancelled)
    );
    let actual_2 = polls.get();

    assert!(actual);
    assert_eq!(actual_2, checkpoint + 1);
}

#[rstest::rstest]
#[case("value", " = 0\n")]
#[case("import dependency", " as alias\n")]
fn project_summary_bounds_accumulate_across_individually_valid_modules(
    #[case] prefix: &str,
    #[case] suffix: &str,
) {
    let source = (0..=MAX_SUMMARY_ENTRIES / 2).fold(String::new(), |mut source, index| {
        write!(source, "{prefix}{index}{suffix}").unwrap();
        source
    });
    let sources = [
        (Utf8PathBuf::from("first.py"), source.clone()),
        (Utf8PathBuf::from("second.py"), source),
    ];

    let actual = matches!(
        ExceptionIndex::from_sources(&sources, &[], &|| false),
        Err(AnalysisError::HierarchyLimit)
    );

    assert!(actual);
}

fn run_collection(
    index: &ExceptionIndex,
    path: &Utf8Path,
    module: &ModModule,
    cancelled: &dyn Fn() -> bool,
    sink: &mut dyn FnMut(TextRange, String) -> Result<(), AnalysisError>,
) -> Result<HierarchyReport, AnalysisError> {
    index.collect(path, module, 100, &cancelled, sink)
}

fn recording(
    candidates: &mut Vec<String>,
) -> impl FnMut(TextRange, String) -> Result<(), AnalysisError> + '_ {
    |_, replacement| {
        candidates.push(replacement);
        Ok(())
    }
}

#[rstest::rstest]
#[case("raise __Child()", vec!["Parent"])]
#[case("class Handler:\n def process(self):\n  raise __Child()", vec![])]
fn private_module_exception_name_respects_class_name_mangling(
    #[case] site: &str,
    #[case]
    #[by_ref]
    expected: &[&str],
) {
    let path = Utf8Path::new("sample.py");
    let source = format!("class Parent(Exception): pass\nclass __Child(Parent): pass\n{site}\n");
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.clone())], &[], &|| false).unwrap();
    let private_alias_visible = index.visible[path]
        .values()
        .flatten()
        .any(|(name, _)| name == "__Child");
    let parsed = parse_module(&source).unwrap();
    let mut candidates = Vec::new();
    run_collection(
        &index,
        path,
        parsed.syntax(),
        &|| false,
        &mut recording(&mut candidates),
    )
    .unwrap();
    let replacements = candidates;

    assert!(private_alias_visible);
    assert_eq!(replacements, expected);
}

#[test]
fn exception_ancestry_lookup_is_bounded_for_a_long_acyclic_chain() {
    let path = Utf8Path::new("sample.py");
    let source = (0..=MAX_STEPS).fold(
        String::from("class Root(Exception): pass\n"),
        |mut source, index| {
            let parent = index
                .checked_sub(1)
                .map_or_else(|| "Root".to_owned(), |parent| format!("Child{parent}"));
            writeln!(source, "class Child{index}({parent}): pass").unwrap();
            source
        },
    );
    let index = ExceptionIndex::from_sources(&[(path.to_owned(), source)], &[], &|| false).unwrap();

    let actual = index
        .resolve_class(&(path.to_owned(), format!("Child{MAX_STEPS}")))
        .is_none();

    assert!(actual);
}

#[rstest::rstest]
#[case("raise Child()", vec!["Parent"])]
#[case("try:\n pass\nexcept:\n pass", vec![])]
#[case("class Handler:\n pass\n def process(self):\n  raise Child()", vec!["Parent"])]
#[case("class Handler[T]:\n def process(self):\n  raise Child()", vec![])]
#[case("def process[T]():\n raise Child()", vec![])]
#[case("computed = {**row for row in rows}\nraise Child()", vec!["Parent"])]
#[case("computed = {row for row in rows}\nraise Child()", vec!["Parent"])]
#[case("computed = (row for row in rows)\nraise Child()", vec!["Parent"])]
#[case("computed = {(Child := row) for row in rows}\nraise Child()", vec![])]
fn exception_collection_respects_generic_scopes_and_bare_handlers(
    #[case] site: &str,
    #[case]
    #[by_ref]
    expected: &[&str],
) {
    let path = Utf8Path::new("sample.py");
    let source = format!("class Parent(Exception): pass\nclass Child(Parent): pass\n{site}\n");
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.clone())], &[], &|| false).unwrap();
    let parsed = parse_module(&source).unwrap();
    let mut candidates = Vec::new();
    let report = run_collection(
        &index,
        path,
        parsed.syntax(),
        &|| false,
        &mut recording(&mut candidates),
    )
    .unwrap();

    assert_eq!(report.emitted, candidates.len());
    assert_eq!(candidates, expected);
}

#[test]
fn exception_collection_propagates_an_error_from_the_shared_sink() {
    let path = Utf8Path::new("sample.py");
    let source = "class Parent(Exception): pass\nclass Child(Parent): pass\nraise Child()";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();
    let parsed = parse_module(source).unwrap();

    let actual = matches!(
        run_collection(&index, path, parsed.syntax(), &|| false, &mut |_, _| Err(
            AnalysisError::HierarchyLimit
        )),
        Err(AnalysisError::HierarchyLimit)
    );

    assert!(actual);
}

#[rstest::rstest]
fn exception_collection_stops_during_private_alias_and_method_scans(
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31
    )]
    checkpoint: usize,
) {
    let path = Utf8Path::new("sample.py");
    let source = "class Parent(Exception): pass\nclass Child(Parent): pass\nclass __Private(Parent): pass\nclass Handler:\n def process(self):\n  raise Child()\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();
    let parsed = parse_module(source).unwrap();
    let total = std::cell::Cell::new(0);
    let mut complete = Vec::new();
    run_collection(
        &index,
        path,
        parsed.syntax(),
        &|| {
            total.set(total.get() + 1);
            false
        },
        &mut recording(&mut complete),
    )
    .unwrap();
    let complete_replacements = (complete).clone();
    let polls = std::cell::Cell::new(0);
    let mut partial = Vec::new();
    let result = run_collection(
        &index,
        path,
        parsed.syntax(),
        &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        },
        &mut recording(&mut partial),
    );
    let cancelled = result.is_err();
    let retained_prefix = complete.starts_with(&partial);

    assert_eq!(complete_replacements, ["Parent"]);
    assert_eq!(cancelled, checkpoint < total.get());
    assert!(retained_prefix);
}

#[rstest::rstest]
#[case("[(alias := origin) for first in rows for second in rows if alias]\nalias.member = 1\n")]
#[case("first = origin\nsecond = first\nfirst = second\nsecond.member = 1\n")]
#[case("def update():\n global alias\n alias = origin\n alias.member = 1\n")]
fn escaped_import_mutation_follows_comprehension_global_and_cyclic_aliases(#[case] mutation: &str) {
    let source = format!("import external as origin\n{mutation}");
    let parsed = parse_module(&source).unwrap();
    let module = Module::parse(parsed.syntax()).unwrap();
    let usage = module
        .dependencies
        .get(&("external".to_owned(), 0))
        .unwrap();

    assert!(usage.mutated);
}

#[rstest::rstest]
#[case("class Target(Later): pass\nclass Later(Target): pass\n")]
#[case("exec(code)\nclass Target(Exception): pass\n")]
#[case("Target = 1\n")]
fn ancestry_resolution_rejects_cycles_dynamic_modules_and_nonclass_targets(#[case] source: &str) {
    let path = Utf8Path::new("sample.py");
    let parsed = parse_module(source).unwrap();
    let module = Module::parse(parsed.syntax()).unwrap();
    let index = ExceptionIndex {
        modules: [(path.to_owned(), module)].into(),
        ..ExceptionIndex::default()
    };

    let actual = index
        .resolve_class(&(path.to_owned(), "Target".into()))
        .is_none();

    assert!(actual);
}

#[derive(Clone, Copy, Debug)]
enum IndexPhase {
    Relative,
    Mutated,
    Collision,
}

fn unfinished_index() -> ExceptionIndex {
    let sources = [
        ("pkg/__init__.py", "class Root(Exception): pass\n"),
        (
            "pkg/mod.py",
            "from . import Root\nimport pkg as absolute\nabsolute.Root = custom\nimport builtins as b\nb.Exception = custom\nimport missing.child.leaf\n",
        ),
        (
            "pkg/other.py",
            "from .mod import Root\nclass Child(Root): pass\n",
        ),
    ];
    let paths = sources.map(|(path, _)| Utf8PathBuf::from(path));
    let mut builder = ExceptionIndexBuilder::new(&paths, &[Utf8PathBuf::from(".")], &[], &|| false);
    for (path, source) in sources {
        builder.add_source(Utf8Path::new(path), source, &|| false);
    }
    builder.pending.unwrap().index
}

fn run_index_phase(
    index: &mut ExceptionIndex,
    phase: IndexPhase,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), AnalysisCancelled> {
    match phase {
        IndexPhase::Relative => index.resolve_relative_imports(&cancelled),
        IndexPhase::Mutated => index.exclude_mutated_imports(&cancelled),
        IndexPhase::Collision => index.exclude_submodule_collisions(&cancelled),
    }
}

#[rstest::rstest]
#[case(IndexPhase::Relative, 0)]
#[case(IndexPhase::Relative, 1)]
#[case(IndexPhase::Relative, 2)]
#[case(IndexPhase::Relative, 3)]
#[case(IndexPhase::Relative, 4)]
#[case(IndexPhase::Relative, 5)]
#[case(IndexPhase::Relative, 6)]
#[case(IndexPhase::Relative, 7)]
#[case(IndexPhase::Relative, 8)]
#[case(IndexPhase::Mutated, 0)]
#[case(IndexPhase::Mutated, 1)]
#[case(IndexPhase::Mutated, 2)]
#[case(IndexPhase::Collision, 0)]
#[case(IndexPhase::Collision, 1)]
fn hierarchy_index_phases_observe_cancellation_at_each_pending_step(
    #[case] phase: IndexPhase,
    #[case] checkpoint: usize,
) {
    let mut index = unfinished_index();
    let polls = std::cell::Cell::new(0);

    let actual = run_index_phase(&mut index, phase, &|| {
        let current = polls.get();
        polls.set(current + 1);
        current >= checkpoint
    });
    let actual_2 = polls.get();

    assert_eq!(actual, Err(AnalysisCancelled));
    assert_eq!(actual_2, checkpoint + 1);
}

#[test]
fn mutation_of_builtins_invalidates_every_prepared_module_summary() {
    let mut index = unfinished_index();
    let all_static_before = index.modules.values().all(|module| !module.dynamic);
    run_index_phase(&mut index, IndexPhase::Mutated, &|| false).unwrap();
    let all_dynamic_after = index.modules.values().all(|module| module.dynamic);

    assert!(all_static_before);
    assert!(all_dynamic_after);
}

#[test]
fn relative_and_absolute_dependency_summaries_merge_without_losing_mutation() {
    let mut index = unfinished_index();
    let module = index.modules.get(Utf8Path::new("pkg/mod.py")).unwrap();
    let relative_present_before = module.dependencies.contains_key(&(String::new(), 1));
    let absolute_mutated_before = module.dependencies.get(&("pkg".into(), 0)).unwrap().mutated;
    run_index_phase(&mut index, IndexPhase::Relative, &|| false).unwrap();
    let module = index.modules.get(Utf8Path::new("pkg/mod.py")).unwrap();
    let relative_removed_after = !module.dependencies.contains_key(&(String::new(), 1));
    let merged = module.dependencies.get(&("pkg".into(), 0)).unwrap();
    let merged_eager = merged.eager;
    let merged_mutated = merged.mutated;

    assert!(relative_present_before);
    assert!(absolute_mutated_before);
    assert!(relative_removed_after);
    assert!(merged_eager);
    assert!(merged_mutated);
}

#[rstest::rstest]
fn constructible_descendants_require_constructible_resolved_parents(
    #[values("Exception", "ValueError")] builtin: &str,
    #[values(false, true)] root_constructor: bool,
    #[values(false, true)] child_constructor: bool,
    #[values(false, true)] sibling_constructor: bool,
) {
    let bodies = ["pass", "def __init__(self): pass"];
    let source = format!(
        "class Root({builtin}):\n {}\nclass Child(Root):\n {}\nclass Sibling(Root):\n {}\nclass Grandchild(Child): pass\n",
        bodies[usize::from(root_constructor)],
        bodies[usize::from(child_constructor)],
        bodies[usize::from(sibling_constructor)]
    );
    let index =
        ExceptionIndex::from_sources(&[("sample.py".into(), source)], &[], &|| false).unwrap();

    let actual = index.classes.len();
    let actual_2 = index.children.len();
    let actual_3 = index.children.iter().all(|(parent, children)| {
        !children.has_constructible()
            || index
                .classes
                .get(parent)
                .is_some_and(|parent| parent.constructible)
    });

    assert_eq!(actual, 4);
    assert_eq!(actual_2, 2);
    assert!(actual_3);
}

#[test]
fn one_import_statement_stops_extracting_aliases_after_the_summary_limit() {
    let source = format!(
        "import {}",
        (0..MAX_SUMMARY_ENTRIES + 2)
            .map(|index| format!("module{index} as shared"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let parsed = parse_module(&source).unwrap();
    let mut escapes = Escapes::default();
    escapes.visit_body(&parsed.syntax().body);

    let actual = escapes.imports.len();
    let actual_2 = escapes.import_aliases.len();

    assert!(escapes.limit_exceeded);
    assert_eq!(actual, MAX_SUMMARY_ENTRIES + 1);
    assert_eq!(actual_2, MAX_SUMMARY_ENTRIES + 1);
}

#[rstest::rstest]
#[case("sample.py", "p.Root", usize::MAX, true)]
#[case("sample.py", "Direct", usize::MAX, true)]
#[case("sample.py", "p.", usize::MAX, false)]
#[case("sample.py", "Direct.attribute", usize::MAX, false)]
#[case("sample.py", "p.Unknown", usize::MAX, false)]
#[case("sample.py", "p.Root", 0, false)]
#[case("missing.py", "p.Root", usize::MAX, false)]
#[case("sample.py", "", usize::MAX, false)]
fn imported_class_identity_requires_a_present_loaded_class_spelling(
    #[case] path: &str,
    #[case] spelling: &str,
    #[case] before: usize,
    #[case] resolved: bool,
) {
    let sources = [
        (
            "pkg/__init__.py".into(),
            "class Root(Exception): pass\n".into(),
        ),
        (
            "sample.py".into(),
            "import pkg as p\nfrom pkg import Root as Direct\n".into(),
        ),
    ];
    let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();

    let actual = index.binding_id(Utf8Path::new(path), spelling, before);

    assert_eq!(
        actual,
        resolved.then(|| (Utf8PathBuf::from("pkg/__init__.py"), "Root".to_owned()))
    );
}

#[rstest::rstest]
#[case("missing.py", "Target")]
#[case("sample.py", "Unknown")]
fn ancestry_lookup_rejects_an_unregistered_class_identity(#[case] path: &str, #[case] name: &str) {
    let index = ExceptionIndex::from_sources(
        &[("sample.py".into(), "class Target(Exception): pass\n".into())],
        &[],
        &|| false,
    )
    .unwrap();

    let actual = index
        .classes
        .contains_key(&(Utf8PathBuf::from("sample.py"), "Target".to_owned()));
    let actual_2 = index.resolve_class(&(path.into(), name.into())).is_none();

    assert!(actual);
    assert!(actual_2);
}

#[test]
fn relative_import_without_a_package_context_loses_class_identity() {
    let path = Utf8Path::new("sample.py");
    let source = "from .errors import Parent as Imported\nclass Child(Imported): pass\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();
    let module = index.modules.get(path).unwrap();

    let actual = matches!(
        module.bindings.get("Imported").unwrap().kind,
        BindingKind::Unknown
    );
    let actual_2 = index.classes.is_empty();

    assert!(actual);
    assert!(actual_2);
}

#[test]
fn cancellation_stops_private_alias_discovery_before_the_second_visible_spelling() {
    let path = Utf8Path::new("service.py");
    let source = "from errors import Parent as Public, Child as __Hidden\nclass C:\n def f(self):\n  raise __Hidden()\n";
    let sources = [
        (
            "errors.py".into(),
            "class Parent(Exception): pass\nclass Child(Parent): pass\n".into(),
        ),
        (path.to_owned(), source.to_owned()),
    ];
    let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
    let visible_alias_count = index
        .visible
        .get(path)
        .unwrap()
        .values()
        .map(Vec::len)
        .sum::<usize>();
    let parsed = parse_module(source).unwrap();
    let mut replacements = Vec::new();
    lookup_tests::PRIVATE_ALIAS_VISITS.set(0);
    let result = run_collection(
        &index,
        path,
        parsed.syntax(),
        &|| lookup_tests::PRIVATE_ALIAS_VISITS.get() >= 1,
        &mut recording(&mut replacements),
    );
    let cancelled = matches!(result, Err(AnalysisError::Cancelled));
    let alias_visit_count = lookup_tests::PRIVATE_ALIAS_VISITS.get();
    let replacements = replacements;

    assert_eq!(visible_alias_count, 2);
    assert!(cancelled);
    assert_eq!(alias_visit_count, 1);
    assert_eq!(replacements, [] as [String; 0]);
}
