//! Statement traversal shared by analyzer consumers of Ruff's ordinary visitor.
//!
//! The pinned upstream walker visits elif conditions twice. Only If dispatch is
//! replaced here; the remaining traversal and visitor hooks stay upstream.
//! Implementations that can enter statements must override `visit_stmt` to call
//! this module's `walk_stmt`: the re-exported trait's default still calls Ruff.

#[allow(unused_imports)]
pub(crate) use ruff_python_ast::visitor::*;

/// Walk a statement once, preserving upstream child order and suite callbacks.
pub(crate) fn walk_stmt<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    statement: &'a ruff_python_ast::Stmt,
) {
    if let ruff_python_ast::Stmt::If(statement) = statement {
        visitor.visit_expr(&statement.test);
        visitor.visit_body(&statement.body);
        for clause in &statement.elif_else_clauses {
            // Upstream dispatch uses the walker directly, rather than the
            // optional clause hook; preserve that behavior without a pre-visit.
            walk_elif_else_clause(visitor, clause);
        }
    } else {
        ruff_python_ast::visitor::walk_stmt(visitor, statement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_ast::{Expr, Stmt};
    use ruff_text_size::Ranged;
    use std::fmt::Write as _;

    #[derive(Default)]
    struct Names(Vec<(String, usize)>);

    impl<'a> Visitor<'a> for Names {
        fn visit_stmt(&mut self, statement: &'a Stmt) {
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'a Expr) {
            if let Expr::Name(name) = expression {
                self.0.push((name.id.to_string(), name.start().to_usize()));
            }
            walk_expr(self, expression);
        }
    }

    fn elif_fixture(clauses: usize, nots: usize, nested: bool) -> (String, Vec<(String, usize)>) {
        let mut source = format!("if {}condition0:\n    body0\n", "not ".repeat(nots));
        let mut expected_names = vec!["condition0".to_owned(), "body0".to_owned()];
        for clause in 1..=clauses {
            writeln!(
                source,
                "elif {}condition{clause}:\n    body{clause}",
                "not ".repeat(nots)
            )
            .unwrap();
            expected_names.extend([format!("condition{clause}"), format!("body{clause}")]);
        }
        if nested {
            source.push_str("else:\n    while looping:\n        if inner:\n            first\n        elif not other:\n            second\n        else:\n            last\n");
            expected_names.extend(
                ["looping", "inner", "first", "other", "second", "last"].map(str::to_owned),
            );
        }
        let expected: Vec<_> = expected_names
            .into_iter()
            .map(|name| {
                let start = source.find(&name).unwrap();
                (name, start)
            })
            .collect();
        (source, expected)
    }

    #[rstest::rstest]
    #[case::empty_plain(0, 0, false)]
    #[case::empty_nested(0, 0, true)]
    #[case::empty_negated(0, 3, false)]
    #[case::empty_negated_nested(0, 3, true)]
    #[case::four_plain(4, 0, false)]
    #[case::four_nested(4, 0, true)]
    #[case::four_negated(4, 3, false)]
    #[case::four_negated_nested(4, 3, true)]
    fn elif_visitor_preserves_original_boundary_cases(
        #[case] clauses: usize,
        #[case] nots: usize,
        #[case] nested: bool,
    ) {
        let (source, expected) = elif_fixture(clauses, nots, nested);
        let parsed = ruff_python_parser::parse_module(&source).unwrap();

        let mut actual = Names::default();

        actual.visit_body(&parsed.syntax().body);

        assert_eq!(actual.0, expected, "{source}");
    }

    proptest::proptest! {
        #[test]
        fn elif_conditions_and_nested_suites_visit_each_name_once_in_order(
            clauses in 0usize..=32,
            nots in 0usize..=16,
            nested in proptest::bool::ANY,
        ) {
            let (source, expected) = elif_fixture(clauses, nots, nested);
            let parsed = ruff_python_parser::parse_module(&source).unwrap();

            let mut actual = Names::default();

            actual.visit_body(&parsed.syntax().body);

            proptest::prop_assert_eq!(actual.0, expected, "{}", source);
        }
    }

    #[rstest::rstest]
    #[case::function("def function():\n", "", vec![])]
    #[case::class("class Class:\n", "", vec![])]
    #[case::while_loop("while looping:\n", "", vec!["looping"])]
    #[case::for_loop("for item in iterable:\n", "", vec!["iterable", "item"])]
    #[case::with_statement("with manager:\n", "", vec!["manager"])]
    #[case::try_finally("try:\n", "finally:\n    pass\n", vec![])]
    fn elif_inside_delegated_statement_kinds_visits_each_name_once(
        #[case] prefix: &str,
        #[case] suffix: &str,
        #[case] headers: Vec<&str>,
    ) {
        let suite = "if first:\n    body\nelif not second:\n    other\nelse:\n    last\n";
        let mut indented = String::new();
        for line in suite.lines() {
            writeln!(indented, "    {line}").unwrap();
        }
        let source = format!("{prefix}{indented}{suffix}");
        let expected: Vec<_> = headers
            .into_iter()
            .chain(["first", "body", "second", "other", "last"])
            .map(|name| (name.to_owned(), source.find(name).unwrap()))
            .collect();
        let parsed = ruff_python_parser::parse_module(&source).unwrap();

        let mut actual = Names::default();

        actual.visit_body(&parsed.syntax().body);

        assert_eq!(actual.0, expected, "{source}");
    }

    #[test]
    fn source_order_elif_traversal_already_visits_each_condition_once() {
        use ruff_python_ast::AnyNodeRef;
        use source_order::{SourceOrderVisitor, TraversalSignal};
        struct SourceNames(Vec<(String, usize)>);
        impl<'a> SourceOrderVisitor<'a> for SourceNames {
            fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
                if let AnyNodeRef::ExprName(name) = node {
                    self.0.push((name.id.to_string(), name.start().to_usize()));
                }
                TraversalSignal::Traverse
            }
        }
        let source = "if first:\n    body\nelif not second:\n    other\nelse:\n    last\n";
        let expected: Vec<_> = ["first", "body", "second", "other", "last"]
            .map(|name| (name.to_owned(), source.find(name).unwrap()))
            .into();
        let parsed = ruff_python_parser::parse_module(source).unwrap();

        let mut actual = SourceNames(Vec::new());

        actual.visit_body(&parsed.syntax().body);

        assert_eq!(actual.0, expected);
    }
}
