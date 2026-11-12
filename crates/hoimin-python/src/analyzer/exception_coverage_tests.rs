use super::*;
use ruff_python_parser::parse_module;

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
    assert_eq!(range, TextRange::new(2.into(), 5.into()));
    assert!(diagnostic.contains("unsupported_expression=2"));
}

#[test]
fn lexical_bindings_include_patterns_handlers_and_comprehension_walruses() {
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
    for name in [
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
        "root",
    ] {
        assert!(bindings.names.contains(name), "missing {name}");
    }
    assert!(bindings.dynamic);
    assert!(!bindings.names.contains("x"));
}

#[test]
fn definition_headers_bind_decorators_annotations_bases_and_keywords() {
    let parsed = parse_module(
        "@(decorator := identity)\ndef function(arg: (annotation := int) = (default := 1)) -> (result := int):\n    body_only = 1\n\
         @(class_decorator := identity)\nclass Child((base := Parent), metaclass=(meta := type)):\n    class_body = 1\n",
    ).unwrap();
    let function = header_bindings(&parsed.syntax().body[0]);
    for name in ["decorator", "annotation", "default", "result"] {
        assert!(function.names.contains(name), "missing {name}");
    }
    assert!(!function.names.contains("body_only"));
    let class = header_bindings(&parsed.syntax().body[1]);
    for name in ["class_decorator", "base", "meta"] {
        assert!(class.names.contains(name), "missing {name}");
    }
    assert!(!class.names.contains("class_body"));
    let mut combined = Bindings::default();
    combined.visit_body(&parsed.syntax().body);
    assert!(combined.names.contains("function"));
    assert!(combined.names.contains("Child"));
    assert!(
        header_bindings(&parse_module("pass\n").unwrap().syntax().body[0])
            .names
            .is_empty()
    );
}

#[test]
fn relative_module_handles_absolute_and_above_root_imports() {
    assert_eq!(
        relative_module(Utf8Path::new("pkg/mod.py"), "pkg.mod", "external", 0),
        Some("external".into())
    );
    assert_eq!(
        relative_module(Utf8Path::new("pkg/mod.py"), "pkg.mod", "child", 1),
        Some("pkg.child".into())
    );
    assert_eq!(
        relative_module(Utf8Path::new("pkg/mod.py"), "pkg.mod", "child", 3),
        None
    );
}

#[test]
fn hierarchy_collection_propagates_emission_failure_and_cancellation_at_each_checkpoint() {
    let path = Utf8Path::new("sample.py");
    let source = "class Root(Exception): pass\nclass Child(Root): pass\nclass Sibling(Root): pass\nclass Container:\n    def method(self):\n        raise Child()\ndef f():\n    try:\n        raise Child()\n    except Root:\n        pass\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();
    let parsed = parse_module(source).unwrap();
    let failed = index.collect(path, parsed.syntax(), 100, &|| false, |_, _| {
        Err(AnalysisError::HierarchySourceChanged)
    });
    assert!(matches!(failed, Err(AnalysisError::HierarchySourceChanged)));
    let mut completed = false;
    for checkpoint in 0..4096 {
        let polls = std::cell::Cell::new(0);
        let cancelled = || {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        };
        let mut emitted = 0;
        let result = index.collect(path, parsed.syntax(), 100, &cancelled, |_, _| {
            emitted += 1;
            Ok(())
        });
        match result {
            Ok(report) => {
                assert_eq!(report.emitted, emitted);
                assert!(emitted > 0);
                completed = true;
            }
            Err(error) => assert_eq!(error, AnalysisError::Cancelled),
        }
        if completed {
            break;
        }
    }
    assert!(completed);
}

#[test]
fn builtin_base_requires_visible_ordinary_builtin_import() {
    let path = Utf8Path::new("sample.py");
    let source = "import builtins as b\nfrom builtins import Exception as E\nfrom builtins import object as O\nclass Root(E): pass\nAlias = Root\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();
    assert_eq!(
        index.builtin_base(path, "b.Exception", source.len()),
        Some("Exception".into())
    );
    assert_eq!(
        index.builtin_base(path, "E", source.len()),
        Some("Exception".into())
    );
    assert_eq!(index.builtin_base(path, "E", 0), None);
    assert_eq!(index.builtin_base(path, "E.member", source.len()), None);
    assert_eq!(index.builtin_base(path, "O", source.len()), None);
    assert_eq!(index.builtin_base(path, "Root", source.len()), None);
    assert_eq!(
        index.builtin_base(Utf8Path::new("missing.py"), "Exception", source.len()),
        None
    );
    assert!(
        index
            .resolve_class(&(path.to_owned(), "Alias".into()))
            .is_none()
    );
    let source = "exec(code)\n";
    let index =
        ExceptionIndex::from_sources(&[(path.to_owned(), source.into())], &[], &|| false).unwrap();
    assert_eq!(index.builtin_base(path, "Exception", source.len()), None);
}
