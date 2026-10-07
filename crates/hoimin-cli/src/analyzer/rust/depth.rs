//! Bound recursive analysis independently of candidate retention, and dispose rejected trees.

use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, ModModule};
#[cfg(test)]
use ruff_python_ast::{AtomicNodeIndex, Expr, InterpolatedStringElement, Stmt};

#[cfg(test)]
use ruff_text_size::TextRange;

use super::AnalysisError;

/// The module is depth 1; every child reported by Ruff's source-order visitor adds 1,
/// including auxiliary nodes (parameters, type parameters, patterns and string elements).
pub(crate) const MAX_ANALYSIS_DEPTH: usize = 128;

pub(super) fn check(
    module: &ModModule,
    cancelled: &impl Fn() -> bool,
) -> Result<(), AnalysisError> {
    struct Children<'a, 'b> {
        pending: &'b mut Vec<(AnyNodeRef<'a>, usize)>,
        depth: usize,
    }
    impl<'a> SourceOrderVisitor<'a> for Children<'a, '_> {
        fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
            self.pending.push((node, self.depth));
            // Ruff still invokes leave_node on Skip; no depth counter is mutated there.
            TraversalSignal::Skip
        }
    }
    let mut pending = vec![(AnyNodeRef::ModModule(module), 1)];
    while let Some((node, depth)) = pending.pop() {
        if cancelled() {
            return Err(AnalysisError::Cancelled);
        }
        if depth > MAX_ANALYSIS_DEPTH {
            return Err(AnalysisError::DepthExceeded {
                limit: MAX_ANALYSIS_DEPTH,
            });
        }
        // This dispatch visits the node's children, without entering the node again.
        node.visit_source_order(&mut Children {
            pending: &mut pending,
            depth: depth + 1,
        });
    }
    Ok(())
}

/// Detach recursive edges before dropping each node. Ruff's exhaustive Transformer
/// dispatch owns the variant inventory. Every cycle in its traversal passes through
/// a statement, expression, pattern or interpolated element; these callbacks enqueue
/// ownership instead of recursing. The remaining auxiliary paths have fixed depth.
/// Do not replace this with ordinary drop: a 20,000-term binary AST overflows 2 MiB.
pub(super) fn dispose(module: ModModule) {
    ruff_python_parser::ast_cleanup::dispose_module(module);
}

#[cfg(test)]
mod tests {
    use super::super::{AnalyzeRequest, analyze_source_cancellable};
    use super::*;
    use camino::Utf8Path;
    use hoimin_core::{MutationOperatorSelection, MutationProfile};

    fn expression(terms: usize) -> String {
        vec!["1"; terms].join("+")
    }

    #[test]
    fn exact_depth_boundary_preserves_real_candidates_on_two_mib_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let operators = MutationOperatorSelection::default();
                let request = AnalyzeRequest {
                    path: Utf8Path::new("calc.py"),
                    lines: &[],
                    symbols: &[],
                    operators: &operators,
                    profile: MutationProfile::Full,
                    max_candidates: 1,
                };
                // Module (1), assignment (2), 125 binary nodes, numeric leaf (128).
                let accepted = format!("value = {}\n", expression(126));
                let result = analyze_source_cancellable(&request, &accepted, || false).unwrap();
                assert_eq!(result.candidates.len(), 1);
                assert!(result.truncated);
                assert_eq!(result.candidates[0].operator, "integer_literal_neighbor");
                let rejected = format!("value = {}\n", expression(127));
                assert!(matches!(
                    analyze_source_cancellable(&request, &rejected, || false),
                    Err(AnalysisError::DepthExceeded { limit: 128 })
                ));
                let flat = "value = 1 + 2\n".repeat(2_000);
                let result = analyze_source_cancellable(&request, &flat, || false).unwrap();
                assert_eq!(result.candidates.len(), 1);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn depth_covers_annotations_type_parameters_strings_patterns_and_statements() {
        let deep = expression(130);
        let mut statements = String::new();
        for indent in 0..130 {
            statements.push_str(&" ".repeat(indent));
            statements.push_str("if True:\n");
        }
        statements.push_str(&" ".repeat(130));
        statements.push_str("pass\n");
        let sources = [
            format!("value: {deep}\n"),
            format!("def f[T: {deep}]():\n    return 1 + 2\n"),
            format!("def f[T = {deep}]():\n    return 1 + 2\n"),
            format!("value = f'{{{deep}}}'\n"),
            format!("value = t'{{{deep}}}'\n"),
            format!("value = f'{{x:{{{deep}}}}}'\n"),
            format!(
                "match value:\n    case {}1{}:\n        pass\n",
                "C(".repeat(130),
                ")".repeat(130)
            ),
            statements,
        ];
        for source in sources {
            let parsed = ruff_python_parser::parse_unchecked_source(
                &source,
                ruff_python_ast::PySourceType::Python,
            );
            assert!(
                parsed.has_valid_syntax(),
                "fixture: {}",
                &source[..source.len().min(100)]
            );
            let result = check(parsed.syntax(), &|| false);
            dispose(parsed.into_syntax());
            assert_eq!(result, Err(AnalysisError::DepthExceeded { limit: 128 }));
        }
    }

    #[test]
    fn auxiliary_paths_accept_supported_sources_and_reclaim_owned_trees() {
        let source = r#"
@decorate(1 + 2)
def f[T: list[int] = list[int]](a: list[int] = [1], *b: int, c: int = 2, **d: int) -> list[int]:
    try:
        with manager(1) as resource:
            return [x + 1 for x in a if x > 0]
    except Exception as error:
        raise error
    finally:
        pass
class C[T](Base, metaclass=Meta):
    pass
match value:
    case C([1, *rest], key={"x": x}) as whole if x > 0:
        value = f"{x:{width}}" + t"{x:{width}}"
"#;
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        assert_eq!(check(parsed.syntax(), &|| false), Ok(()));
        dispose(parsed.into_syntax());
        let operators = MutationOperatorSelection::default();
        let request = AnalyzeRequest {
            path: Utf8Path::new("calc.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 1_000,
        };
        assert_ne!(
            analyze_source_cancellable(&request, source, || false)
                .unwrap()
                .candidates,
            Vec::new()
        );
    }

    #[test]
    fn nested_annotation_boundary_runs_annotation_candidates_on_two_mib_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let mut operators = MutationOperatorSelection::default();
                operators.include(hoimin_core::MutationOperator::TypeListSequence);
                let request = AnalyzeRequest {
                    path: Utf8Path::new("types.py"),
                    lines: &[],
                    symbols: &[],
                    operators: &operators,
                    profile: MutationProfile::Full,
                    max_candidates: 1_000,
                };
                // Module + annotated assignment + 125 subscripts + innermost name.
                let source = format!(
                    "from typing import Sequence\nvalue: {}int{}\n",
                    "list[".repeat(125),
                    "]".repeat(125)
                );
                let result = analyze_source_cancellable(&request, &source, || false).unwrap();
                assert!(
                    result
                        .candidates
                        .iter()
                        .any(|candidate| candidate.operator == "type_list_sequence")
                );
                let rejected = format!("value: {}int{}\n", "list[".repeat(126), "]".repeat(126));
                assert!(matches!(
                    analyze_source_cancellable(&request, &rejected, || false),
                    Err(AnalysisError::DepthExceeded { limit: 128 })
                ));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn disposal_cuts_nested_format_specification_ownership() {
        // Synthetic ownership stress, not a claim about Python's format grammar.
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                for source in ["value = f'{x}'", "value = t'{x}'"] {
                    let mut module = ruff_python_parser::parse_module(source)
                        .unwrap()
                        .into_syntax();
                    let Stmt::Assign(assign) = &mut module.body[0] else {
                        panic!("assignment fixture")
                    };
                    let elements = match assign.value.as_mut() {
                        Expr::FString(expr) => {
                            let ruff_python_ast::FStringPart::FString(part) =
                                expr.value.iter_mut().next().unwrap()
                            else {
                                panic!("f-string fixture")
                            };
                            &mut part.elements
                        }
                        Expr::TString(expr) => &mut expr.value.iter_mut().next().unwrap().elements,
                        _ => panic!("interpolated string fixture"),
                    };
                    let template = elements.iter().next().unwrap().clone();
                    let mut nested = template.clone();
                    for _ in 0..25_000 {
                        let mut parent = template.clone();
                        let InterpolatedStringElement::Interpolation(interpolation) = &mut parent
                        else {
                            panic!("interpolation fixture")
                        };
                        interpolation.format_spec =
                            Some(Box::new(ruff_python_ast::InterpolatedStringFormatSpec {
                                range: TextRange::default(),
                                node_index: AtomicNodeIndex::default(),
                                elements: vec![nested].into(),
                            }));
                        nested = parent;
                    }
                    *elements = vec![nested].into();
                    assert_eq!(
                        check(&module, &|| false),
                        Err(AnalysisError::DepthExceeded { limit: 128 })
                    );
                    dispose(module);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn cancellation_before_and_after_parse_retains_distinct_reason() {
        let source = format!("value = {}\n", expression(25_000));
        let operators = MutationOperatorSelection::default();
        let request = AnalyzeRequest {
            path: Utf8Path::new("calc.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 1,
        };
        for cancel_on in [0, 1, 20] {
            let calls = std::cell::Cell::new(0);
            let result = analyze_source_cancellable(&request, &source, || {
                let call = calls.get();
                calls.set(call + 1);
                call >= cancel_on
            });
            assert!(matches!(result, Err(AnalysisError::Cancelled)));
        }
    }
}
