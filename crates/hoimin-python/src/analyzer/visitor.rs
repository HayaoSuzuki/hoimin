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

    #[test]
    fn elif_conditions_and_nested_suites_visit_each_name_once_in_order() {
        for clauses in 0..=4 {
            for nots in 0..=3 {
                for nested in [false, true] {
                    let mut source = format!("if {}condition0:\n    body0\n", "not ".repeat(nots));
                    let mut expected_names = vec!["condition0".to_owned(), "body0".to_owned()];
                    for clause in 1..=clauses {
                        writeln!(
                            source,
                            "elif {}condition{clause}:\n    body{clause}",
                            "not ".repeat(nots)
                        )
                        .unwrap();
                        expected_names
                            .extend([format!("condition{clause}"), format!("body{clause}")]);
                    }
                    if nested {
                        source.push_str("else:\n    while looping:\n        if inner:\n            first\n        elif not other:\n            second\n        else:\n            last\n");
                        expected_names.extend(
                            ["looping", "inner", "first", "other", "second", "last"]
                                .map(str::to_owned),
                        );
                    }
                    let expected: Vec<_> = expected_names
                        .into_iter()
                        .map(|name| {
                            let start = source.find(&name).unwrap();
                            (name, start)
                        })
                        .collect();
                    let parsed = ruff_python_parser::parse_module(&source).unwrap();
                    let mut actual = Names::default();

                    actual.visit_body(&parsed.syntax().body);

                    assert_eq!(actual.0, expected, "{source}");
                }
            }
        }
    }

    #[test]
    fn elif_inside_delegated_statement_kinds_visits_each_name_once() {
        let suite = "if first:\n    body\nelif not second:\n    other\nelse:\n    last\n";
        let mut indented = String::new();
        for line in suite.lines() {
            writeln!(indented, "    {line}").unwrap();
        }
        for (prefix, suffix, headers) in [
            ("def function():\n", "", vec![]),
            ("class Class:\n", "", vec![]),
            ("while looping:\n", "", vec!["looping"]),
            ("for item in iterable:\n", "", vec!["iterable", "item"]),
            ("with manager:\n", "", vec!["manager"]),
            ("try:\n", "finally:\n    pass\n", vec![]),
        ] {
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
