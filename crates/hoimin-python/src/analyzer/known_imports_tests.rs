use super::{
    AnnotationImports, KnownImports, NameResolutionIndex, contains_disallowed_annotation,
    normalize_binding_flow_imports,
};
use proptest::prelude::*;
use ruff_python_ast::Stmt;
use ruff_python_parser::parse_module;

const BINDINGS: [(&str, &str); 4] = [
    (
        "from typing import Sequence as name\n",
        "direct:name=typing.Sequence",
    ),
    ("import typing as name\n", "module:name=typing"),
    (
        "from typing import TypeVar\nname = TypeVar('name')\ndel TypeVar\n",
        "type-var:name",
    ),
    (
        "from collections.abc import Sequence as name\n",
        "direct:name=collections.abc.Sequence",
    ),
];

fn imports_from(source: &str) -> KnownImports {
    let parsed = parse_module(source).unwrap();
    let mut imports = KnownImports::default();
    for statement in &parsed.syntax().body {
        imports.transfer_statement(statement);
    }
    imports
}

#[test]
fn rebinding_replaces_the_previous_category_and_payload() {
    for (before, _) in BINDINGS {
        for (after, expected) in BINDINGS {
            // Arrange: every category can precede every other category.
            let source = format!("{before}{after}");

            // Act.
            let actual = imports_from(&source);

            // Assert: stale category or payload must not survive rebinding.
            assert_eq!(
                normalize_binding_flow_imports(&actual),
                [expected],
                "{source}"
            );
        }
    }
}

#[test]
fn intersection_requires_the_same_category_and_payload() {
    for (left_source, left_fact) in BINDINGS {
        for (right_source, right_fact) in BINDINGS {
            // Arrange.
            let left = imports_from(left_source);
            let right = imports_from(right_source);
            let expected = if left_fact == right_fact {
                vec![left_fact]
            } else {
                vec![]
            };

            // Act.
            let actual = KnownImports::intersection([left, right]).unwrap();

            // Assert: matching names alone do not establish a known binding.
            assert_eq!(normalize_binding_flow_imports(&actual), expected);
        }
    }
}

#[test]
fn copying_a_name_replaces_or_removes_its_previous_binding() {
    for (before, _) in BINDINGS {
        for (source_text, expected) in BINDINGS.into_iter().chain([("", "")]) {
            // Arrange.
            let mut destination = imports_from(before);
            let source = imports_from(source_text);
            let expected = if expected.is_empty() {
                vec![]
            } else {
                vec![expected]
            };

            // Act.
            destination.copy_name_from(&source, "name");

            // Assert: absence in the source must also overwrite the destination.
            assert_eq!(normalize_binding_flow_imports(&destination), expected);
        }
    }
}

#[test]
fn wildcard_retains_only_modules_for_known_providers() {
    for (provider, expected) in [
        ("typing", vec!["module:t=typing"]),
        ("collections.abc", vec!["module:t=typing"]),
        ("helpers", vec![]),
        (".typing", vec![]),
    ] {
        // Arrange.
        let source = format!(
            "import typing as t\nfrom typing import Sequence, TypeVar\nT = TypeVar('T')\nfrom {provider} import *\n"
        );

        // Act.
        let actual = imports_from(&source);

        // Assert: recognized wildcard imports preserve module aliases only.
        assert_eq!(
            normalize_binding_flow_imports(&actual),
            expected,
            "{provider}"
        );
    }
}

#[test]
fn unqualified_spelling_prefers_direct_alias_then_lexical_order() {
    for (source, expected) in [
        (
            "import typing as a\nfrom typing import Sequence as z\n",
            "z",
        ),
        ("from typing import Sequence as z, Sequence as b\n", "b"),
        ("import typing as z\nimport typing as b\n", "b.Sequence"),
    ] {
        // Arrange.
        let imports = imports_from(source);

        // Act.
        let actual = imports.spelling_for("list", &["typing.Sequence"]);

        // Assert: category priority precedes lexical ordering.
        assert_eq!(actual.as_deref(), Some(expected));
    }
}

#[test]
fn type_variables_with_scalar_names_are_disallowed_annotations() {
    for name in ["str", "int", "float", "bool", "bytes"] {
        // Arrange.
        let imports = imports_from(&format!(
            "from typing import TypeVar\n{name} = TypeVar('{name}')\n"
        ));
        let resolution = NameResolutionIndex::default();
        let annotation_imports = AnnotationImports {
            eager_imports_available: true,
            known: &imports,
            resolution: &resolution,
            offset: 0,
        };
        let parsed = parse_module(&format!("value: {name}\n")).unwrap();
        let Stmt::AnnAssign(statement) = &parsed.syntax().body[0] else {
            panic!("annotation fixture");
        };

        // Act.
        let actual = contains_disallowed_annotation(&statement.annotation, &annotation_imports);

        // Assert: the scalar spelling cannot bypass TypeVar exclusion.
        assert!(actual, "{name}");
    }
}

#[test]
fn type_variables_are_not_treated_as_known_imports_for_stability() {
    // Arrange: without a recorded name occurrence, treating T as an import rejects it.
    let imports = imports_from("from typing import TypeVar\nT = TypeVar('T')\n");
    let resolution = NameResolutionIndex::default();
    let annotation_imports = AnnotationImports {
        eager_imports_available: true,
        known: &imports,
        resolution: &resolution,
        offset: 0,
    };

    // Act.
    let stable = annotation_imports.stable_name("T");

    // Assert.
    assert!(stable);
}

fn binding_states() -> impl Strategy<Value = KnownImports> {
    prop::collection::btree_map(0_u8..8, 0_u8..5, 0..8).prop_map(|entries| {
        let mut source = String::new();
        for (name, category) in entries {
            let statement = match category {
                0 => format!("from typing import Sequence as n{name}\n"),
                1 => format!("from collections.abc import Sequence as n{name}\n"),
                2 => format!("import typing as n{name}\n"),
                3 => format!("import collections.abc as n{name}\n"),
                _ => format!(
                    "from typing import TypeVar\nn{name} = TypeVar('n{name}')\ndel TypeVar\n"
                ),
            };
            source.push_str(&statement);
        }
        imports_from(&source)
    })
}

proptest! {
    #[test]
    fn intersection_is_idempotent(state in binding_states()) {
        // Arrange.
        let mut actual = state.clone();


    // Act.
        actual.intersect_with(&state);


    // Assert.
        prop_assert_eq!(actual, state);
    }

    #[test]
    fn intersection_is_commutative(left in binding_states(), right in binding_states()) {
        // Arrange.
        let mut forward = left.clone();
        let mut backward = right.clone();


    // Act.
        forward.intersect_with(&right);
        backward.intersect_with(&left);


    // Assert.
        prop_assert_eq!(forward, backward);
    }

    #[test]
    fn intersection_is_associative(a in binding_states(), b in binding_states(), c in binding_states()) {
        // Arrange.
        let mut left = a.clone();
        let mut right = a;
        let mut tail = b.clone();


    // Act.
        left.intersect_with(&b);
        left.intersect_with(&c);
        tail.intersect_with(&c);
        right.intersect_with(&tail);


    // Assert.
        prop_assert_eq!(left, right);
    }
}

thread_local! {
    pub(super) static QUALIFIED_BINDING_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[test]
fn qualified_spelling_work_is_independent_of_unrelated_imports() {
    for imports_count in [1, 32, 256] {
        use std::fmt::Write as _;
        let mut source = String::new();
        for i in 0..imports_count {
            writeln!(source, "import typing as t{i}").unwrap();
        }
        let imports = imports_from(&source);
        for annotations in [1, 16, 64] {
            QUALIFIED_BINDING_VISITS.set(0);
            for _ in 0..annotations {
                assert_eq!(
                    imports
                        .spelling_for("t0.List", &["typing.Sequence"])
                        .as_deref(),
                    Some("t0.Sequence")
                );
            }
            assert!(
                QUALIFIED_BINDING_VISITS.get() <= annotations,
                "imports={imports_count}, annotations={annotations}, visits={}",
                QUALIFIED_BINDING_VISITS.get()
            );
        }
    }
}

#[test]
fn qualified_spelling_preserves_provider_prefix_and_shadowing() {
    for (source, spelling, expected) in [
        ("import typing as t\n", "t.List", Some("t.Sequence")),
        ("import typing as t10\n", "t1.List", None),
        (
            "import collections.abc\n",
            "collections.abc.List",
            Some("collections.abc.Sequence"),
        ),
        (
            "import collections.abc as abc\n",
            "abc.List",
            Some("abc.Sequence"),
        ),
        ("import typing as t\nt = 1\n", "t.List", None),
        ("from typing import List as t\n", "t.List", None),
    ] {
        let imports = imports_from(source);
        let targets = ["typing.Sequence", "collections.abc.Sequence"];
        assert_eq!(
            imports.spelling_for(spelling, &targets).as_deref(),
            expected,
            "{source}"
        );
        assert_eq!(
            imports.spelling_for_where(spelling, &targets, |_| false),
            None
        );
    }
}
