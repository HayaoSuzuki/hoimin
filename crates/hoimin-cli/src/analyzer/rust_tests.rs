use super::{
    AnalyzeRequest, AnalyzerCandidate, CandidatePrefix, LineIndex, analyze_source,
    analyze_source_cancellable,
};
use crate::analyzer::AnalyzerDiagnosticCode;
use camino::Utf8Path;
use hoimin_core::{
    ByteSpan, LineRange, MutationOperator, MutationOperatorSelection, MutationProfile,
};
use proptest::prelude::*;
use ruff_python_parser::parse_module;

fn analyze(source: &str) -> super::AnalyzerOutput {
    analyze_with(Utf8Path::new("pkg/sample.py"), &[], &[], 10_000, source)
}

fn apply_candidate_and_reparse(source: &str, candidate: &super::AnalyzerCandidate) -> String {
    let start = usize::try_from(candidate.span.start).expect("candidate start fits usize");
    let length = usize::try_from(candidate.span.length).expect("candidate length fits usize");
    let end = start
        .checked_add(length)
        .expect("candidate end does not overflow");
    let mut mutated = source.to_owned();
    mutated.replace_range(start..end, &candidate.replacement);
    assert!(
        parse_module(&mutated).is_ok(),
        "candidate {} produced invalid Python: {mutated}",
        candidate.operator
    );
    mutated
}

fn assert_type_list_sequence_sites(source: &str, expected: &[(u64, u64, u32, Option<&str>, &str)]) {
    let output = analyze_types(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.span.length,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.replacement.as_str(),
            )
        })
        .collect();
    assert_eq!(actual, expected);
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

fn prefix_candidate(start: u64, replacement: &str, operator: &str) -> AnalyzerCandidate {
    AnalyzerCandidate {
        path: Utf8Path::new("pkg/sample.py").to_path_buf(),
        span: ByteSpan { start, length: 1 },
        original: "original".to_owned(),
        replacement: replacement.to_owned(),
        operator: operator.to_owned(),
        line: 1,
        column: 0,
        symbol: None,
    }
}

fn candidate_starts(candidates: &[AnalyzerCandidate]) -> Vec<u64> {
    candidates
        .iter()
        .map(|candidate| candidate.span.start)
        .collect()
}

#[test]
fn candidate_prefix_retains_the_earliest_k_plus_one_unique_candidates() {
    let mut prefix = CandidatePrefix::new(2);
    assert_eq!(prefix.capacity, 3);

    for candidate in [
        prefix_candidate(9, "nine", "operator"),
        prefix_candidate(1, "one", "operator"),
        prefix_candidate(5, "five", "operator"),
        prefix_candidate(3, "three", "operator"),
        prefix_candidate(1, "one", "operator"),
    ] {
        prefix.push(candidate);
    }

    assert_eq!(prefix.identities.len(), 3);
    let result = prefix.finish();
    assert_eq!(candidate_starts(&result.candidates), vec![1, 3, 5]);
    assert!(result.overflowed);
    assert_eq!(result.candidates.len(), 3);
}

#[test]
fn candidate_prefix_keeps_emission_order_for_equal_sort_keys() {
    let mut prefix = CandidatePrefix::new(2);
    for candidate in [
        prefix_candidate(1, "first", "operator"),
        prefix_candidate(1, "second", "operator"),
        prefix_candidate(1, "third", "operator"),
    ] {
        prefix.push(candidate);
    }

    let result = prefix.finish();
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| candidate.replacement.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "third"]
    );
    assert!(!result.overflowed);
}

#[test]
fn candidate_prefix_with_zero_limit_retains_one_earliest_candidate() {
    let mut prefix = CandidatePrefix::new(0);
    assert_eq!(prefix.capacity, 1);
    prefix.push(prefix_candidate(3, "three", "operator"));
    prefix.push(prefix_candidate(3, "three", "operator"));
    prefix.push(prefix_candidate(1, "one", "operator"));

    let result = prefix.finish();
    assert_eq!(candidate_starts(&result.candidates), vec![1]);
    assert!(result.overflowed);
    assert_eq!(result.candidates.len(), 1);
}

#[test]
fn candidate_prefix_saturates_capacity_at_usize_maximum() {
    let prefix = CandidatePrefix::new(usize::MAX);

    assert_eq!(prefix.capacity, usize::MAX);
}

#[test]
fn exception_handler_type_tuples_do_not_emit_collection_candidates() {
    let source = concat!(
        "before_list = [before]\n",
        "before_tuple = (before,)\n",
        "try:\n    work()\n",
        "except (ValueError, TypeError):\n",
        "    handler_list = [handler]\n",
        "    handler_tuple = (handler,)\n",
        "    try:\n        work()\n",
        "    except ((KeyError, IndexError)):\n",
        "        nested_list = [nested]\n",
        "        nested_tuple = (nested,)\n",
        "try:\n    work()\n",
        "except* (ValueError, TypeError):\n",
        "    starred_list = [starred]\n",
        "    starred_tuple = (starred,)\n",
        "after_list = [after]\n",
        "after_tuple = (after,)\n",
    );
    let output = analyze(source);
    let collection_candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_list_tuple")
        .collect();

    assert!(collection_candidates.iter().all(|candidate| {
        !matches!(
            candidate.original.as_str(),
            "(ValueError, TypeError)" | "(KeyError, IndexError)"
        )
    }));

    let actual: Vec<_> = collection_candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.span,
            )
        })
        .collect();
    let expected: Vec<_> = [
        ("[before]", "(before,)"),
        ("(before,)", "[before,]"),
        ("[handler]", "(handler,)"),
        ("(handler,)", "[handler,]"),
        ("[nested]", "(nested,)"),
        ("(nested,)", "[nested,]"),
        ("[starred]", "(starred,)"),
        ("(starred,)", "[starred,]"),
        ("[after]", "(after,)"),
        ("(after,)", "[after,]"),
    ]
    .into_iter()
    .map(|(original, replacement)| {
        (
            original,
            replacement,
            ByteSpan {
                start: source
                    .find(original)
                    .expect("literal fixture contains the expected source span")
                    as u64,
                length: original.len() as u64,
            },
        )
    })
    .collect();
    assert_eq!(actual, expected);

    for candidate in collection_candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn exception_handler_type_builtin_calls_are_excluded_for_ordinary_and_starred_handlers() {
    for (source, expected) in [
        (
            "try:\n    work()\nexcept tuple((ValueError, TypeError)):\n    handler = tuple((1, 2))\n",
            vec![
                (
                    "tuple",
                    "list",
                    ByteSpan {
                        start: 69,
                        length: 5,
                    },
                ),
                (
                    "(1, 2)",
                    "[1, 2]",
                    ByteSpan {
                        start: 75,
                        length: 6,
                    },
                ),
            ],
        ),
        (
            "try:\n    work()\nexcept* tuple((ValueError, TypeError)):\n    handler = tuple((1, 2))\n",
            vec![
                (
                    "tuple",
                    "list",
                    ByteSpan {
                        start: 70,
                        length: 5,
                    },
                ),
                (
                    "(1, 2)",
                    "[1, 2]",
                    ByteSpan {
                        start: 76,
                        length: 6,
                    },
                ),
            ],
        ),
    ] {
        let output = analyze(source);
        let collection_candidates: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "collection_list_tuple")
            .collect();
        let actual: Vec<_> = collection_candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.original.as_str(),
                    candidate.replacement.as_str(),
                    candidate.span,
                )
            })
            .collect();

        assert_eq!(actual, expected, "source: {source:?}");
        for candidate in collection_candidates {
            apply_candidate_and_reparse(source, candidate);
        }
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the fixture records every curated safe exception replacement and exact span"
)]
fn exception_type_pair_candidates_are_curated_and_syntax_directed() {
    let source = concat!(
        "before_list = [before]\n",
        "try:\n    work()\n",
        "except ValueError:\n    pass\n",
        "except TypeError:\n    pass\n",
        "except KeyError:\n    pass\n",
        "except IndexError:\n    pass\n",
        "except AttributeError:\n    pass\n",
        "except FileNotFoundError:\n    pass\n",
        "except PermissionError:\n    pass\n",
        "except ConnectionError:\n    pass\n",
        "except TimeoutError:\n    pass\n",
        "except ImportError:\n    pass\n",
        "except ModuleNotFoundError:\n    pass\n",
        "except ZeroDivisionError:\n    pass\n",
        "except OverflowError:\n    pass\n",
        "after_tuple = (after,)\n",
    );
    let output = analyze(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "exception_type_pair")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.span,
            )
        })
        .collect();
    assert_eq!(
        actual,
        vec![
            (
                "ValueError",
                "TypeError",
                ByteSpan {
                    start: 46,
                    length: 10,
                },
            ),
            (
                "TypeError",
                "ValueError",
                ByteSpan {
                    start: 74,
                    length: 9,
                },
            ),
            (
                "KeyError",
                "IndexError",
                ByteSpan {
                    start: 101,
                    length: 8,
                },
            ),
            (
                "KeyError",
                "AttributeError",
                ByteSpan {
                    start: 101,
                    length: 8,
                },
            ),
            (
                "IndexError",
                "KeyError",
                ByteSpan {
                    start: 127,
                    length: 10,
                },
            ),
            (
                "AttributeError",
                "KeyError",
                ByteSpan {
                    start: 155,
                    length: 14,
                },
            ),
            (
                "FileNotFoundError",
                "PermissionError",
                ByteSpan {
                    start: 187,
                    length: 17,
                },
            ),
            (
                "PermissionError",
                "FileNotFoundError",
                ByteSpan {
                    start: 222,
                    length: 15,
                },
            ),
            (
                "ConnectionError",
                "TimeoutError",
                ByteSpan {
                    start: 255,
                    length: 15,
                },
            ),
            (
                "TimeoutError",
                "ConnectionError",
                ByteSpan {
                    start: 288,
                    length: 12,
                },
            ),
            (
                "ImportError",
                "ModuleNotFoundError",
                ByteSpan {
                    start: 318,
                    length: 11,
                },
            ),
            (
                "ModuleNotFoundError",
                "ImportError",
                ByteSpan {
                    start: 347,
                    length: 19,
                },
            ),
            (
                "ZeroDivisionError",
                "OverflowError",
                ByteSpan {
                    start: 384,
                    length: 17,
                },
            ),
            (
                "OverflowError",
                "ZeroDivisionError",
                ByteSpan {
                    start: 419,
                    length: 13,
                },
            ),
        ]
    );
    for candidate in output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "exception_type_pair")
    {
        apply_candidate_and_reparse(source, candidate);
    }

    let unsupported = concat!(
        "try:\n    work()\n",
        "except (ValueError, TypeError):\n    pass\n",
        "except module.Error:\n    pass\n",
        "except make_error():\n    pass\n",
    );
    assert!(
        analyze(unsupported)
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "exception_type_pair")
    );

    let starred = "try:\n    work()\nexcept* ValueError:\n    pass\n";
    assert!(parse_module(starred).is_ok(), "except* fixture parses");
    assert!(
        analyze(starred)
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "exception_type_pair")
    );

    let nested = concat!(
        "try:\n    work()\n",
        "except* ValueError:\n",
        "    try:\n        work()\n",
        "    except TypeError:\n        pass\n",
    );
    assert!(
        analyze(nested)
            .candidates
            .iter()
            .any(|candidate| candidate.original == "TypeError")
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the fixture covers every explicit risky exception operator and parseability"
)]
fn exception_risky_candidates_require_explicit_selection_and_reparse() {
    let source = concat!(
        "before_list = [before]\n",
        "try:\n    work()\n",
        "except:\n    pass\n",
        "try:\n    work()\n",
        "except Exception:\n    pass\n",
        "try:\n    work()\n",
        "except BaseException:\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError,):\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError, TypeError,):\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError, # keep this comment\n",
        "        TypeError,):\n    pass\n",
        "after_tuple = (after,)\n",
    );
    let default_output = analyze(source);
    assert!(default_output.candidates.iter().all(|candidate| {
        !matches!(
            candidate.operator.as_str(),
            "exception_bare_to_exception"
                | "exception_exception_to_bare"
                | "exception_base_boundary"
                | "exception_tuple_add_pair"
                | "exception_tuple_remove_member"
        )
    }));

    let mut operators = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::ExceptionBareToException,
        MutationOperator::ExceptionExceptionToBare,
        MutationOperator::ExceptionBaseBoundary,
        MutationOperator::ExceptionTupleAddPair,
        MutationOperator::ExceptionTupleRemoveMember,
    ] {
        operators.include(operator);
    }
    let output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    let risky_candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.starts_with("exception_"))
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
                candidate.span,
            )
        })
        .collect();
    assert_eq!(
        risky_candidates,
        vec![
            (
                "except",
                "except Exception",
                "exception_bare_to_exception",
                ByteSpan {
                    start: 39,
                    length: 6,
                },
            ),
            (
                "Exception",
                "BaseException",
                "exception_base_boundary",
                ByteSpan {
                    start: 79,
                    length: 9,
                },
            ),
            (
                "Exception",
                "",
                "exception_exception_to_bare",
                ByteSpan {
                    start: 79,
                    length: 9,
                },
            ),
            (
                "BaseException",
                "Exception",
                "exception_base_boundary",
                ByteSpan {
                    start: 122,
                    length: 13,
                },
            ),
            (
                "(ValueError,)",
                "(ValueError, TypeError)",
                "exception_tuple_add_pair",
                ByteSpan {
                    start: 169,
                    length: 13,
                },
            ),
            (
                "(ValueError, TypeError,)",
                "( TypeError,)",
                "exception_tuple_remove_member",
                ByteSpan {
                    start: 216,
                    length: 24,
                },
            ),
            (
                "(ValueError, TypeError,)",
                "(ValueError, )",
                "exception_tuple_remove_member",
                ByteSpan {
                    start: 216,
                    length: 24,
                },
            ),
            (
                "(ValueError, # keep this comment\n        TypeError,)",
                "( # keep this comment\n        TypeError,)",
                "exception_tuple_remove_member",
                ByteSpan {
                    start: 274,
                    length: 52,
                },
            ),
            (
                "(ValueError, # keep this comment\n        TypeError,)",
                "(ValueError, # keep this comment\n        )",
                "exception_tuple_remove_member",
                ByteSpan {
                    start: 274,
                    length: 52,
                },
            ),
        ]
    );
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_bare_to_exception"
            && candidate.original == "except"
            && candidate.replacement == "except Exception"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_exception_to_bare"
            && candidate.original == "Exception"
            && candidate.replacement.is_empty()
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_base_boundary"
            && candidate.original == "Exception"
            && candidate.replacement == "BaseException"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_base_boundary"
            && candidate.original == "BaseException"
            && candidate.replacement == "Exception"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_tuple_add_pair"
            && candidate.original == "(ValueError,)"
            && candidate.replacement == "(ValueError, TypeError)"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_tuple_remove_member"
            && candidate.original == "(ValueError, TypeError,)"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_tuple_remove_member"
            && candidate.original.contains("# keep this comment")
    }));
    for candidate in &output.candidates {
        if candidate.operator.starts_with("exception_") {
            let mutated = apply_candidate_and_reparse(source, candidate);
            if candidate.original.contains("# keep this comment") {
                assert!(mutated.contains("# keep this comment"));
            }
        }
    }

    let unsupported = concat!(
        "ValueError = Custom\n",
        "try:\n    work()\n",
        "except ValueError:\n    pass\n",
        "except module.Error:\n    pass\n",
        "except make_error():\n    pass\n",
        "except (ValueError, module.Error):\n    pass\n",
        "except* BaseException:\n    pass\n",
    );
    assert!(
        analyze_source(
            &AnalyzeRequest {
                path: Utf8Path::new("pkg/sample.py"),
                lines: &[],
                symbols: &[],
                operators: &operators,
                profile: MutationProfile::Full,
                max_candidates: 10_000,
            },
            unsupported,
        )
        .candidates
        .iter()
        .all(|candidate| !candidate.operator.starts_with("exception_"))
    );

    let shadowed = concat!(
        "TypeError = CustomError\n",
        "Exception = CustomException\n",
        "BaseException = CustomBaseException\n",
        "try:\n    work()\n",
        "except ValueError:\n    pass\n",
        "try:\n    work()\n",
        "except:\n    pass\n",
        "try:\n    work()\n",
        "except Exception:\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError,):\n    pass\n",
    );
    let shadowed_output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        shadowed,
    );
    assert!(
        shadowed_output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.starts_with("exception_")),
        "unexpected shadowed exception candidates: {:#?}",
        shadowed_output.candidates
    );

    let handler_position = concat!(
        "try:\n    work()\n",
        "except Exception as error:\n    pass\n",
        "try:\n    work()\n",
        "except Exception:\n    pass\n",
        "except TypeError:\n    pass\n",
        "try:\n    work()\n",
        "except TypeError:\n    pass\n",
        "except Exception:\n    pass\n",
    );
    let positioned_output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        handler_position,
    );
    assert_eq!(
        positioned_output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "exception_exception_to_bare")
            .count(),
        1
    );

    for termination in ["SystemExit", "KeyboardInterrupt", "GeneratorExit"] {
        let source = format!("try:\n    work()\nexcept ({termination}, ValueError):\n    pass\n");
        let output = analyze_source(
            &AnalyzeRequest {
                path: Utf8Path::new("pkg/sample.py"),
                lines: &[],
                symbols: &[],
                operators: &operators,
                profile: MutationProfile::Full,
                max_candidates: 10_000,
            },
            &source,
        );
        for candidate in output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "exception_tuple_remove_member")
        {
            let mutated = apply_candidate_and_reparse(&source, candidate);
            assert!(
                !mutated.contains(&format!("except ({termination}")),
                "tuple removal generated an individual termination handler: {mutated}"
            );
        }
    }
}

#[test]
fn annotations_do_not_emit_default_bitwise_mutations() {
    let output = analyze("value: Left | None\nresult = left & right\n");

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "bitwise_and_or")
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec!["&"]
    );
}

#[test]
fn type_alias_and_match_capture_bindings_shadow_collection_builtins() {
    for source in [
        "type list = int\nresult = list(items)\n",
        "match value:\n    case list:\n        pass\nresult = list(items)\n",
    ] {
        let output = analyze(source);
        assert!(
            output
                .candidates
                .iter()
                .all(|candidate| candidate.operator != "collection_list_tuple"),
            "unexpected list candidate for {source:?}: {:#?}",
            output.candidates
        );
    }
}

#[test]
fn structural_candidates_reject_bare_generators_and_preserve_commented_literals() {
    let source = concat!(
        "items.append(value for value in values)\n",
        "mapping.get(value for value in values)\n",
        "items.insert(0, (yield value))\n",
        "items.insert(0, (yield from values))\n",
        "items = [item, # keep this comment\n]\n",
        "other = [item # keep this comment\n,]\n",
        "yielding_value = [(yield value)]\n",
        "yielding = [(yield from values)]\n",
        "named = [item := value]\n",
    );
    let output = analyze(source);

    assert!(
        output.candidates.iter().all(|candidate| {
            !matches!(
                candidate.operator.as_str(),
                "collection_append_insert" | "structure_mapping_get_subscript"
            )
        }),
        "unexpected candidates: {:#?}",
        output.candidates
    );
    for candidate in output.candidates.iter().filter(|candidate| {
        candidate.operator.starts_with("collection_")
            || candidate.operator.starts_with("structure_")
    }) {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn annotations_and_star_imports_suppress_collection_candidates() {
    let annotation_output = analyze(
        "from typing import Callable\nhandler: Callable[[Left, Right], Result]\nvalue: tuple[Left, Right]\n",
    );
    assert!(
        annotation_output
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple")
    );

    let star_import_output =
        analyze("from helpers import *\nresult = list(items)\nvalue = tuple(items)\n");
    assert!(
        star_import_output
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple")
    );
}

#[test]
fn shadowed_collection_builtins_are_not_mutated_as_calls() {
    let source = "any = custom_any\nfrom helpers import all\ndef list(tuple):\n    min = custom_min\n    for max in items:\n        pass\n    with resource as sorted:\n        pass\n    try:\n        pass\n    except Error as reversed:\n        pass\n    if (frozenset := custom_frozenset):\n        return list(items), tuple(items), set(items), frozenset(items), min(items), max(items), sorted(items), reversed(items)\nclass set:\n    pass\n";
    let output = analyze(source);
    assert!(
        output.candidates.iter().all(|candidate| {
            !matches!(
                candidate.original.as_str(),
                "any"
                    | "all"
                    | "list"
                    | "tuple"
                    | "set"
                    | "frozenset"
                    | "min"
                    | "max"
                    | "sorted"
                    | "reversed"
            )
        }),
        "shadowed builtins must not emit name-replacement candidates: {:#?}",
        output.candidates
    );
}

#[test]
fn sibling_bindings_do_not_suppress_builtin_or_exception_pairs() {
    let source = concat!(
        "def convert(items):\n",
        "    return list(items)\n",
        "def handle():\n",
        "    try:\n",
        "        work()\n",
        "    except ValueError:\n",
        "        recover()\n",
        "def helper():\n",
        "    list = custom_list\n",
        "    tuple = custom_tuple\n",
        "    TypeError = CustomTypeError\n",
    );
    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "collection_list_tuple" | "exception_type_pair"
            )
        })
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        observed,
        [
            ("list", "tuple", 2, Some("convert")),
            ("ValueError", "TypeError", 6, Some("handle")),
        ]
    );
}

#[test]
fn visible_source_or_destination_binding_suppresses_builtin_pairs() {
    let source = concat!(
        "def source_shadowed(items):\n",
        "    result = list(items)\n",
        "    list = custom_list\n",
        "    return result\n",
        "def clean(items):\n",
        "    return list(items)\n",
    );
    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_list_tuple")
        .map(|candidate| (candidate.line, candidate.symbol.as_deref()))
        .collect::<Vec<_>>();

    assert_eq!(observed, [(6, Some("clean"))]);
}

#[test]
fn shadowed_destination_suppresses_an_otherwise_builtin_source() {
    let source = concat!(
        "def convert(items):\n",
        "    tuple = custom_tuple\n",
        "    return list(items)\n",
    );
    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple"),
        "shadowed replacement destination must suppress the candidate: {:#?}",
        output.candidates
    );
}

#[test]
fn module_class_closure_and_directive_resolution_is_scope_aware() {
    let source = concat!(
        "early = list(items)\n",
        "list = custom_list\n",
        "late = list(items)\n",
        "def outer(items):\n",
        "    max = custom_max\n",
        "    def inner():\n",
        "        return max(items)\n",
        "    return inner\n",
        "all = custom_all\n",
        "def redirected(items):\n",
        "    global all\n",
        "    return all(items)\n",
        "def enclosing(items):\n",
        "    min = custom_min\n",
        "    def nested():\n",
        "        nonlocal min\n",
        "        return min(items)\n",
        "    return nested\n",
    );
    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "collection_list_tuple" | "collection_min_max" | "collection_any_all"
            )
        })
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(observed, [("list", "tuple", 1, None)]);

    let class_source = concat!(
        "class Box:\n",
        "    early = tuple(items)\n",
        "    tuple = custom_tuple\n",
        "    late = tuple(items)\n",
        "    def method(self, items):\n",
        "        return tuple(items)\n",
    );
    let class_output = analyze(class_source);
    let class_observed = class_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_list_tuple")
        .map(|candidate| (candidate.line, candidate.symbol.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(class_observed, [(2, Some("Box")), (6, Some("Box.method"))]);
}

#[test]
fn comprehension_exception_target_and_wildcard_boundaries_are_conservative() {
    let source = concat!(
        "outer = [item for item in list(items)]\n",
        "inner = [list(item) for list in factories]\n",
        "before = tuple(items)\n",
        "try:\n",
        "    work()\n",
        "except Error as tuple:\n",
        "    inside = tuple(items)\n",
        "after = tuple(items)\n",
        "def local_target(items):\n",
        "    before = tuple(items)\n",
        "    try:\n",
        "        work()\n",
        "    except Error as tuple:\n",
        "        inside = tuple(items)\n",
        "    after = tuple(items)\n",
    );
    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_list_tuple")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        observed,
        [
            ("list", "tuple", 1, None),
            ("tuple", "list", 3, None),
            ("tuple", "list", 8, None),
        ]
    );

    let wildcard = analyze("from helpers import *\nresult = list(items)\n");
    assert!(
        wildcard
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple")
    );
}

#[test]
fn class_exception_targets_do_not_leak_into_methods() {
    let source = concat!(
        "class Box:\n",
        "    try:\n",
        "        work()\n",
        "    except Error as list:\n",
        "        def class_handler(self, items):\n",
        "            return list(items)\n",
    );
    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_list_tuple")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(observed, [("list", "tuple", 6, Some("Box.class_handler"))]);

    let immediate_class = analyze(concat!(
        "try:\n",
        "    work()\n",
        "except Error as list:\n",
        "    class Immediate:\n",
        "        value = list(items)\n",
    ));
    assert!(
        immediate_class
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple")
    );
}

#[test]
fn imports_are_source_ordered_and_lambda_parameters_are_scope_local() {
    let import_source = concat!(
        "before = list(items)\n",
        "from helpers import list\n",
        "after = list(items)\n",
    );
    let import_output = analyze(import_source);
    let import_observed = import_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_list_tuple")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(import_observed, [("list", "tuple", 1)]);

    let lambda_output = analyze(concat!(
        "clean = lambda items: tuple(items)\n",
        "shadowed = lambda tuple, items: tuple(items)\n",
    ));
    let lambda_observed = lambda_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_list_tuple")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(lambda_observed, [("tuple", "list", 1)]);
}

#[test]
fn dynamic_binding_operations_fail_closed_after_their_possible_effect() {
    let source = concat!(
        "before = list(items)\n",
        "exec(\"list = custom_list\")\n",
        "after = list(items)\n",
    );
    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_list_tuple")
        .map(|candidate| (candidate.original.as_str(), candidate.line))
        .collect::<Vec<_>>();

    assert_eq!(observed, [("list", 1)]);

    let dynamic_namespace = analyze(concat!(
        "globals()[\"tuple\"] = custom_tuple\n",
        "result = list(items)\n",
    ));
    assert!(
        dynamic_namespace
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple")
    );
}

#[test]
fn collection_calls_and_literals_emit_exact_parseable_candidates() {
    let source = concat!(
        "any_result = any(items)\n",
        "all_result = all(items)\n",
        "list_result = list(values)\n",
        "tuple_result = tuple(values)\n",
        "empty_list_call = list()\n",
        "empty_tuple_call = tuple()\n",
        "set_result = set(values)\n",
        "frozen_result = frozenset(values)\n",
        "minimum = min(first, second, key=rank)\n",
        "maximum = max(values, default=fallback)\n",
        "items.append(value)\n",
        "items.insert(0, value)\n",
        "members.add(value)\n",
        "members.discard(value)\n",
        "members.remove(value)\n",
        "text.startswith(prefix)\n",
        "text.endswith(suffix, start, stop)\n",
        "text.split(separator, maxsplit=limit)\n",
        "text.rsplit()\n",
        "many = [first, *rest, last,]\n",
        "one = [item]\n",
        "empty_list = []\n",
        "tuple_many = (first, *rest, last,)\n",
        "tuple_one = (item,)\n",
        "empty_tuple = ()\n",
        "bare_tuple = first, *rest,\n",
    );
    let output = analyze(source);
    let collection_candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.starts_with("collection_"))
        .collect();
    let actual: Vec<_> = collection_candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            ("any", "all", "collection_any_all"),
            ("all", "any", "collection_any_all"),
            ("list", "tuple", "collection_list_tuple"),
            ("tuple", "list", "collection_list_tuple"),
            ("list", "tuple", "collection_list_tuple"),
            ("tuple", "list", "collection_list_tuple"),
            ("set", "frozenset", "collection_set_frozenset"),
            ("frozenset", "set", "collection_set_frozenset"),
            ("min", "max", "collection_min_max"),
            ("max", "min", "collection_min_max"),
            (
                "items.append(value)",
                "items.insert(0, value)",
                "collection_append_insert",
            ),
            (
                "items.insert(0, value)",
                "items.append(value)",
                "collection_append_insert",
            ),
            ("add", "discard", "collection_set_add_discard"),
            ("discard", "add", "collection_set_add_discard"),
            ("discard", "remove", "collection_set_remove_discard"),
            ("remove", "discard", "collection_set_remove_discard"),
            ("startswith", "endswith", "collection_string_starts_ends"),
            ("endswith", "startswith", "collection_string_starts_ends"),
            ("split", "rsplit", "collection_string_split_rsplit"),
            ("rsplit", "split", "collection_string_split_rsplit"),
            (
                "[first, *rest, last,]",
                "(first, *rest, last,)",
                "collection_list_tuple",
            ),
            ("[item]", "(item,)", "collection_list_tuple"),
            ("[]", "()", "collection_list_tuple"),
            (
                "(first, *rest, last,)",
                "[first, *rest, last,]",
                "collection_list_tuple",
            ),
            ("(item,)", "[item,]", "collection_list_tuple"),
            ("()", "[]", "collection_list_tuple"),
            ("first, *rest,", "[first, *rest,]", "collection_list_tuple",),
        ]
    );

    for candidate in collection_candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn collection_excludes_unsupported_call_and_literal_shapes() {
    let source = concat!(
        "list_comp = [item for item in items]\n",
        "set_comp = {item for item in items}\n",
        "left, right = values\n",
        "any()\n",
        "any(first, second)\n",
        "list(iterable=items)\n",
        "tuple(*items)\n",
        "set(iterable=items)\n",
        "frozenset(*items)\n",
        "items.insert(1, value)\n",
        "items.insert(index, value)\n",
        "items.sort(reverse=True)\n",
        "members.add(*values)\n",
        "members.discard(**options)\n",
        "members.remove(value, extra)\n",
        "text.startswith(*parts)\n",
        "text.endswith(**options)\n",
        "text.split(*parts)\n",
        "text.rsplit(**options)\n",
        "set_literal = {first, second}\n",
    );
    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.starts_with("collection_")),
        "unexpected collection candidates: {:#?}",
        output.candidates
    );
}

#[test]
fn structure_calls_emit_exact_parseable_candidates() {
    let source = concat!(
        "appended = items.append(value)\n",
        "extended = items.extend([value])\n",
        "got = mapping.get(key)\n",
        "subscripted = mapping[key]\n",
        "sorted_items = items.sort()\n",
        "reversed_items = items.reverse()\n",
        "ordered = sorted(items)\n",
        "flipped = reversed(items)\n",
    );
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.starts_with("structure_"))
        .collect();
    let actual: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                "items.append(value)",
                "items.extend([value])",
                "structure_append_extend",
            ),
            (
                "items.extend([value])",
                "items.append(value)",
                "structure_append_extend",
            ),
            (
                "mapping.get(key)",
                "mapping[key]",
                "structure_mapping_get_subscript",
            ),
            (
                "mapping[key]",
                "mapping.get(key)",
                "structure_mapping_get_subscript",
            ),
            ("items.sort()", "items.reverse()", "structure_sort_reverse",),
            ("items.reverse()", "items.sort()", "structure_sort_reverse",),
            ("sorted", "reversed", "structure_sorted_reversed"),
            ("reversed", "sorted", "structure_sorted_reversed"),
        ]
    );

    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_replacements_preserve_nested_sources_and_reparse() {
    let source = concat!(
        "appended = items().append(\n",
        "    \"value\"  # retained\n",
        ")\n",
        "extended = items().extend([\n",
        "    \"value\"  # retained\n",
        "])\n",
        "combined = container.mapping.get((make_key(\"field\"))) + 1  # retained\n",
        "subscripted = container.mapping[(make_key(\"field\"))]\n",
        "sorted_result = sorted(produce_items())\n",
        "reversed_result = reversed(produce_items())\n",
    );
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.starts_with("structure_"))
        .collect();
    let actual: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                "items().append(\n    \"value\"  # retained\n)",
                "items().extend([\n    \"value\"  # retained\n])",
                "structure_append_extend",
            ),
            (
                "items().extend([\n    \"value\"  # retained\n])",
                "items().append(\n    \"value\"  # retained\n)",
                "structure_append_extend",
            ),
            (
                "container.mapping.get((make_key(\"field\")))",
                "container.mapping[(make_key(\"field\"))]",
                "structure_mapping_get_subscript",
            ),
            (
                "container.mapping[(make_key(\"field\"))]",
                "container.mapping.get((make_key(\"field\")))",
                "structure_mapping_get_subscript",
            ),
            ("sorted", "reversed", "structure_sorted_reversed"),
            ("reversed", "sorted", "structure_sorted_reversed"),
        ]
    );

    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_candidates_skip_type_annotation_expressions() {
    let source = concat!(
        "value: mapping[key]\n",
        "fallback: mapping.get(key)\n",
        "def annotated(parameter: mapping[key]) -> mapping.get(key):\n",
        "    pass\n",
        "result = mapping[key]\n",
    );
    let output = analyze(source);

    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.starts_with("structure_"))
        .collect();
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec!["mapping[key]"],
    );
    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_excludes_unsupported_shapes_and_receivers() {
    let source = concat!(
        "items.append(*values)\n",
        "items.append(value=value)\n",
        "items.extend(values)\n",
        "items.extend([first, second])\n",
        "items.extend([*values])\n",
        "items.extend([value], extra)\n",
        "items.extend(values=[value])\n",
        "mapping.get(key, fallback)\n",
        "mapping.get(key,)\n",
        "mapping.get(key,  # trailing comma\n",
        ")\n",
        "mapping.get(key=key)\n",
        "mapping.get(*keys)\n",
        "mapping.get(**options)\n",
        "factory().get(key)\n",
        "factory()[key]\n",
        "mapping[first, second]\n",
        "mapping[*keys]\n",
        "mapping[key:stop]\n",
        "mapping[key] = value\n",
        "del mapping[key]\n",
        "items.sort(key=rank)\n",
        "items.sort(reverse=True)\n",
        "items.reverse(*values)\n",
        "items.reverse(values=values)\n",
        "sorted(items, key=rank)\n",
        "sorted(iterable=items)\n",
        "reversed(items, extra)\n",
        "reversed(iterable=items)\n",
        "sorted(*items)\n",
    );
    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.starts_with("structure_")),
        "unexpected structural candidates: {:#?}",
        output.candidates
    );
}

#[test]
fn bitwise_and_or() {
    let source = "and_result = left & right\nor_result = left | right\n";
    let output = analyze(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "bitwise_and_or")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![("&", "|", "bitwise_and_or"), ("|", "&", "bitwise_and_or"),]
    );
}

#[test]
fn bitwise_shift() {
    let source = "left_result = value << amount\nright_result = value >> amount\n";
    let output = analyze(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "bitwise_shift")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![("<<", ">>", "bitwise_shift"), (">>", "<<", "bitwise_shift"),]
    );
}

#[test]
fn structure_index_neighbor_mutates_decimal_load_indices_only() {
    let source = concat!(
        "zero = items[0]\n",
        "one = items[1]\n",
        "largest = items[18446744073709551615]\n",
        "negative = items[-1]\n",
        "expression = items[index + 1]\n",
        "hexadecimal = items[0x10]\n",
        "underscored = items[1_000]\n",
        "annotation: items[1]\n",
        "assigned[1] = value\n",
        "del deleted[1]\n",
    );
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "structure_index_neighbor")
        .collect();
    let actual: Vec<_> = candidates
        .iter()
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();

    assert_eq!(
        actual,
        vec![
            ("0", "1"),
            ("1", "2"),
            ("1", "0"),
            ("18446744073709551615", "18446744073709551614"),
        ]
    );
    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_slice_neighbor_mutates_decimal_bounds_without_zero_steps() {
    let source = concat!(
        "all_bounds = items[1:3:1]\n",
        "empty_start_and_step = items[:3:]\n",
        "empty_bounds = items[:]\n",
        "expression = items[start:stop:step]\n",
        "negative = items[-1:-3:-1]\n",
        "hexadecimal = items[0x10:0x20:0x1]\n",
        "underscored = items[1_000:2_000:3_000]\n",
        "annotation: items[1:3:1]\n",
        "assigned[1:3:1] = values\n",
        "del deleted[1:3:1]\n",
    );
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "structure_slice_neighbor")
        .collect();
    let actual: Vec<_> = candidates
        .iter()
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();

    assert_eq!(
        actual,
        vec![
            ("1", "2"),
            ("1", "0"),
            ("3", "4"),
            ("3", "2"),
            ("1", "2"),
            ("3", "4"),
            ("3", "2"),
        ]
    );
    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn collection_literals_and_inner_token_candidates_remain_individually_parseable() {
    let source = "value = [a == b]\n";
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        candidates,
        vec![
            ("[a == b]", "(a == b,)", "collection_list_tuple"),
            ("==", "!=", "compare_eq_ne"),
        ]
    );
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn candidate_replacements_reparse_as_python() {
    let source = "result = left == right\n";
    let output = analyze(source);
    let candidate = output
        .candidates
        .iter()
        .find(|candidate| candidate.operator == "compare_eq_ne")
        .expect("comparison candidate");

    assert_eq!(
        apply_candidate_and_reparse(source, candidate),
        "result = left != right\n"
    );
}

const TOKEN_OPERATOR_NAMES: &[&str] = &[
    "augmented_add_sub",
    "binary_add_sub",
    "binary_floor_mod",
    "binary_mul_div",
    "bitwise_and_or",
    "bitwise_shift",
    "boolean_and_or",
    "boolean_literal",
    "break_continue",
    "compare_eq_ne",
    "compare_order",
    "identity",
    "membership",
    "remove_not",
    "unary_sign",
];

#[test]
fn token_operator_candidates_cover_supported_ast_roles_and_reparse() {
    let source = concat!(
        "equal = left == right\n",
        "unequal = left != right\n",
        "ordered = first < second <= third > fourth >= fifth\n",
        "member = item in items\n",
        "not_member = item not in items\n",
        "same = left is right\n",
        "not_same = left is not right\n",
        "both = left and right\n",
        "either = left or right\n",
        "sum_value = left + right - extra\n",
        "positive = +value\n",
        "negative = -value\n",
        "product = left * right / divisor\n",
        "remainder = left // right % divisor\n",
        "bits = left & right | extra\n",
        "shifted = left << right >> extra\n",
        "value += increment\n",
        "value -= decrement\n",
        "inverted = not value\n",
        "truth = True\n",
        "falsity = False\n",
        "while active:\n    break\n",
        "while pending:\n    continue\n",
    );
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| TOKEN_OPERATOR_NAMES.contains(&candidate.operator.as_str()))
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        candidates,
        vec![
            ("==", "!=", "compare_eq_ne"),
            ("!=", "==", "compare_eq_ne"),
            ("<", "<=", "compare_order"),
            ("<=", "<", "compare_order"),
            (">", ">=", "compare_order"),
            (">=", ">", "compare_order"),
            ("in", "not in", "membership"),
            ("not in", "in", "membership"),
            ("is", "is not", "identity"),
            ("is not", "is", "identity"),
            ("and", "or", "boolean_and_or"),
            ("or", "and", "boolean_and_or"),
            ("+", "-", "binary_add_sub"),
            ("-", "+", "binary_add_sub"),
            ("+", "-", "unary_sign"),
            ("-", "+", "unary_sign"),
            ("*", "/", "binary_mul_div"),
            ("/", "*", "binary_mul_div"),
            ("//", "%", "binary_floor_mod"),
            ("%", "//", "binary_floor_mod"),
            ("&", "|", "bitwise_and_or"),
            ("|", "&", "bitwise_and_or"),
            ("<<", ">>", "bitwise_shift"),
            (">>", "<<", "bitwise_shift"),
            ("+=", "-=", "augmented_add_sub"),
            ("-=", "+=", "augmented_add_sub"),
            ("not value", "value", "remove_not"),
            ("True", "False", "boolean_literal"),
            ("False", "True", "boolean_literal"),
            ("break", "continue", "break_continue"),
            ("continue", "break", "break_continue"),
        ]
    );
    for candidate in output
        .candidates
        .iter()
        .filter(|candidate| TOKEN_OPERATOR_NAMES.contains(&candidate.operator.as_str()))
    {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn composite_comparisons_preserve_multiline_trivia_and_reparse() {
    for (source, operator, span, original, replacement, mutated) in [
        (
            "result = (\n    item not  # comment\n    in items\n)\n",
            "membership",
            ByteSpan {
                start: 20,
                length: 21,
            },
            "not  # comment\n    in",
            "  # comment\n    in",
            "result = (\n    item   # comment\n    in items\n)\n",
        ),
        (
            "result = (\n    left is  # comment\n    not right\n)\n",
            "identity",
            ByteSpan {
                start: 20,
                length: 21,
            },
            "is  # comment\n    not",
            "is  # comment\n    ",
            "result = (\n    left is  # comment\n     right\n)\n",
        ),
    ] {
        let candidate = analyze(source)
            .candidates
            .into_iter()
            .find(|candidate| candidate.operator == operator)
            .expect("composite comparison candidate");

        assert_eq!(candidate.span, span);
        assert_eq!(candidate.original, original);
        assert_eq!(candidate.replacement, replacement);
        assert_eq!(apply_candidate_and_reparse(source, &candidate), mutated);
    }
}

#[test]
#[ignore = "benchmark harness; run explicitly in release mode"]
fn benchmark_candidate_line_positions() {
    use std::fmt::Write as _;

    let mut source = String::with_capacity(307_200);
    for index in 0..7_680 {
        writeln!(
            source,
            "result_{index:05} = left_{index:05} + right_{index:05}"
        )
        .expect("writing to String cannot fail");
    }
    assert_eq!(source.len(), 307_200);

    let started = std::time::Instant::now();
    let output = analyze(&source);
    let elapsed = started.elapsed();
    let candidates = std::hint::black_box(output).candidates.len();

    assert_eq!(candidates, 7_680);
    println!(
        "source_bytes={} candidates={candidates} elapsed_ms={}",
        source.len(),
        elapsed.as_secs_f64() * 1_000.0
    );
}

#[test]
#[ignore = "benchmark harness; run explicitly in release mode"]
fn benchmark_adversarial_ast_fact_indexes() {
    use std::fmt::Write as _;

    const FUNCTIONS: usize = 512;
    let mut source = String::with_capacity(FUNCTIONS * 180);
    for index in 0..FUNCTIONS {
        writeln!(
            source,
            "def scope_{index:04}(value: list[int] | tuple[int, ...] = [1, 2]):\n    assert not value\n    print(not value)\n    return (not value) and value[0] + 1"
        )
        .expect("writing to String cannot fail");
    }

    let started = std::time::Instant::now();
    let output = analyze_with_profile(MutationProfile::Focused, usize::MAX, &source);
    let elapsed = started.elapsed();
    let stats = output.fact_lookups;

    assert_eq!(stats.annotation.facts, FUNCTIONS);
    assert_eq!(stats.arid.facts, FUNCTIONS * 3);
    assert_eq!(stats.not_operand.facts, FUNCTIONS * 3);
    assert_eq!(stats.scope.facts, FUNCTIONS);
    assert_eq!(stats.not_operand.queries, FUNCTIONS * 3);
    assert_eq!(stats.not_operand.comparisons, 0);
    for index in [stats.annotation, stats.arid, stats.scope] {
        let comparisons_per_query = if index.facts == 0 {
            0
        } else {
            usize::BITS as usize - index.facts.leading_zeros() as usize
        };
        assert!(
            index.comparisons <= index.queries * comparisons_per_query,
            "stats={index:?}",
        );
    }
    assert_eq!(output.candidates.len(), FUNCTIONS * 5);
    assert_eq!(
        output.candidates.first().map(|candidate| (
            candidate.original.as_str(),
            candidate.operator.as_str(),
            candidate.symbol.as_deref(),
        )),
        Some(("not value", "remove_not", Some("scope_0000"))),
    );
    assert_eq!(
        output.candidates.last().map(|candidate| (
            candidate.original.as_str(),
            candidate.operator.as_str(),
            candidate.symbol.as_deref(),
        )),
        Some(("+", "binary_add_sub", Some("scope_0511"))),
    );
    println!(
        "source_bytes={} candidates={} elapsed_ms={} stats={stats:?}",
        source.len(),
        output.candidates.len(),
        elapsed.as_secs_f64() * 1_000.0,
    );
}

#[test]
fn irrelevant_operator_spelling_skips_annotation_index_lookup() {
    let addition = analyze("result = left + right\n");
    let multiplication = analyze("result = left * right\n");
    let bitwise = analyze("result = left & right\n");

    assert_eq!(
        addition.fact_lookups.annotation.queries,
        multiplication.fact_lookups.annotation.queries,
    );
    assert_eq!(
        bitwise.fact_lookups.annotation.queries,
        addition.fact_lookups.annotation.queries + 1,
    );
    assert_eq!(addition.fact_lookups.annotation.comparisons, 0);
    assert_eq!(addition.fact_lookups.annotation.facts, 0);
}

#[test]
fn line_index_reports_one_based_lines_and_unicode_scalar_columns() {
    let source = "alpha\nβeta\r\n終 = left == right\n";
    let line_index = LineIndex::new(source);

    for (offset, expected) in [
        (0, (1, 0)),
        (6, (2, 0)),
        (8, (2, 1)),
        (11, (2, 4)),
        (24, (3, 9)),
    ] {
        assert_eq!(
            line_index.line_and_column(source, offset),
            expected,
            "position at byte offset {offset}",
        );
    }
}

#[test]
fn line_index_positions_token_and_type_annotation_candidates() {
    let source = "from typing import Optional\nπ = left == right\n値: Optional[int]\n";
    let output = analyze_types(source);
    let positions: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.line,
                candidate.column,
            )
        })
        .collect();

    assert_eq!(positions, vec![("==", 2, 9), ("Optional[int]", 3, 3)]);
}

#[test]
fn large_source_analysis_observes_cancellation_during_token_traversal() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let source = "value = left == right\n".repeat(20_000);
    let probes = AtomicUsize::new(0);
    let result = analyze_source_cancellable(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/large.py"),
            lines: &[],
            symbols: &[],
            operators: &MutationOperatorSelection::default(),
            profile: MutationProfile::Full,
            max_candidates: usize::MAX,
        },
        &source,
        || probes.fetch_add(1, Ordering::Relaxed) >= 128,
    );

    assert!(matches!(result, Err(super::AnalysisCancelled)));
}

fn analyze_types(source: &str) -> super::AnalyzerOutput {
    analyze_types_with_profile(MutationProfile::Full, source)
}

fn analyze_with_profile(
    profile: MutationProfile,
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
    let operators = MutationOperatorSelection::default();
    analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile,
            max_candidates,
        },
        source,
    )
}

fn analyze_types_with_profile(profile: MutationProfile, source: &str) -> super::AnalyzerOutput {
    let mut operators = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::TypeNullableRemove,
        MutationOperator::TypeNullableAdd,
        MutationOperator::TypeListSequence,
        MutationOperator::TypeSetAbstractSet,
        MutationOperator::TypeMapping,
        MutationOperator::TypeIterableIterator,
        MutationOperator::TypeSequenceIterable,
    ] {
        operators.include(operator);
    }
    analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile,
            max_candidates: 10_000,
        },
        source,
    )
}
fn analyze_with(
    path: &Utf8Path,
    lines: &[LineRange],
    symbols: &[String],
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
    let operators = MutationOperatorSelection::default();
    analyze_source(
        &AnalyzeRequest {
            path,
            lines,
            symbols,
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates,
        },
        source,
    )
}

fn analyze_with_all_candidate_producers(
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
    let mut operators = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::TypeNullableRemove,
        MutationOperator::TypeNullableAdd,
        MutationOperator::TypeListSequence,
        MutationOperator::TypeSetAbstractSet,
        MutationOperator::TypeMapping,
        MutationOperator::TypeIterableIterator,
        MutationOperator::TypeSequenceIterable,
    ] {
        operators.include(operator);
    }
    analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/high.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates,
        },
        source,
    )
}

#[test]
fn bounded_collection_preserves_the_exact_full_output_prefix_and_retention_bounds() {
    use std::fmt::Write;

    let mut source = String::new();
    for index in 0..100 {
        writeln!(
            source,
            "def value_{index}(items: list[int]) -> list[int]:\n    return list(items[{index}] + {index})"
        )
        .expect("writing to a string succeeds");
    }
    let full = analyze_with_all_candidate_producers(10_000, &source);
    let bounded = analyze_with_all_candidate_producers(3, &source);

    assert!(full.candidates.len() > 3);
    assert_eq!(bounded.candidates, full.candidates[..3]);
    assert!(bounded.truncated);
    assert_eq!(
        bounded.diagnostics[0].code,
        AnalyzerDiagnosticCode::CandidateLimitExceeded
    );
    assert!(
        bounded
            .retention
            .producer_peaks
            .iter()
            .all(|peak| *peak > 0)
    );
    assert_eq!(bounded.retention.producer_peaks, [4; 3]);
    assert_eq!(bounded.retention.merged_peak, 12);
}

#[test]
fn bounded_collection_filters_focused_arid_candidates_before_prefix_capacity() {
    let source = format!("{}result = 1 + 2\n", "print(True)\n".repeat(100));
    let focused = analyze_with_profile(MutationProfile::Focused, 1, &source);

    assert_eq!(focused.candidates.len(), 1);
    assert_eq!(focused.candidates[0].line, 101);
    assert_eq!(focused.candidates[0].operator, "binary_add_sub");
    assert!(!focused.truncated);
    assert!(focused.diagnostics.is_empty());
    assert_eq!(focused.retention.producer_peaks[0], 1);
}

#[test]
fn bounded_collection_with_zero_limit_keeps_only_an_overflow_probe() {
    let output = analyze_with(Utf8Path::new("pkg/zero.py"), &[], &[], 0, "value = 1 + 2\n");

    assert!(output.candidates.is_empty());
    assert!(output.truncated);
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::CandidateLimitExceeded
    );
    assert!(
        output
            .retention
            .producer_peaks
            .iter()
            .all(|peak| *peak <= 1)
    );
    assert!(output.retention.merged_peak <= 3);
}

#[test]
fn focused_profile_suppresses_main_print_assert_and_defaults() {
    let source = "if __name__ == \"__main__\":\n    print(1 + 2)\n    assert 3 == 3\nelse:\n    fallback = 4 + 5\n\ndef f(flag=True, *, enabled=False):\n    return flag + enabled\n";
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
    let descriptors: Vec<_> = focused
        .candidates
        .iter()
        .map(|candidate| (candidate.line, candidate.operator.as_str()))
        .collect();
    assert_eq!(
        descriptors,
        vec![(5, "binary_add_sub"), (8, "binary_add_sub"),]
    );
}

#[test]
fn focused_profile_accepts_only_exact_main_guard_shapes() {
    let source = "if \"__main__\" == __name__:\n    reversed = 1 + 2\nif __name__ != \"__main__\":\n    inequality = 3 + 4\nif __name__ == \"__main__\" == \"__main__\":\n    chained = 5 + 6\nif __name__ == \"entry\":\n    entry = 7 + 8\n";
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
    assert!(
        focused
            .candidates
            .iter()
            .all(|candidate| candidate.line != 2)
    );
    for line in [3, 4, 5, 6, 7, 8] {
        assert!(
            focused
                .candidates
                .iter()
                .any(|candidate| candidate.line == line),
            "expected an eligible candidate on line {line}",
        );
    }
}

#[test]
fn focused_profile_suppresses_only_bare_print_and_assert() {
    let source = "print(1 + 2)\nlogger.print(3 + 4)\nassert 5 + 6\nregular = 7 + 8\n";
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
    let descriptors: Vec<_> = focused
        .candidates
        .iter()
        .map(|candidate| (candidate.line, candidate.operator.as_str()))
        .collect();
    assert_eq!(
        descriptors,
        vec![(2, "binary_add_sub"), (4, "binary_add_sub")]
    );
}

#[test]
fn focused_profile_retains_type_annotation_candidates() {
    let source = "from typing import Optional\n\ndef choose(value: Optional[int], enabled=True) -> Optional[int]:\n    return value\n";
    let focused = analyze_types_with_profile(MutationProfile::Focused, source);
    assert!(
        focused
            .candidates
            .iter()
            .any(|candidate| candidate.operator.starts_with("type_"))
    );
    assert!(
        focused
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "boolean_literal")
    );
}

#[test]
fn focused_filter_runs_before_candidate_limit() {
    let focused = analyze_with_profile(
        MutationProfile::Focused,
        1,
        "def choose(enabled=True):\n    return 1 + 2\n",
    );
    assert_eq!(focused.candidates.len(), 1);
    assert_eq!(focused.candidates[0].line, 2);
    assert_eq!(focused.candidates[0].operator, "binary_add_sub");
}

#[test]
fn focused_profile_applies_after_line_and_symbol_selection() {
    let source = "def selected(enabled=True):\n    return 1 + 2\n\ndef ignored(enabled=True):\n    return 3 + 4\n";
    let lines = vec![LineRange { start: 2, end: 2 }];
    let symbols = vec!["pkg.sample:selected".to_owned()];
    let operators = MutationOperatorSelection::default();
    let focused = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &lines,
            symbols: &symbols,
            operators: &operators,
            profile: MutationProfile::Focused,
            max_candidates: 10_000,
        },
        source,
    );
    let descriptors: Vec<_> = focused
        .candidates
        .iter()
        .map(|candidate| (candidate.line, candidate.operator.as_str()))
        .collect();
    assert_eq!(descriptors, vec![(2, "binary_add_sub")]);
}

#[test]
fn full_profile_matches_default_candidate_output() {
    assert_eq!(
        analyze("result = first + second\n").candidates,
        analyze_with_profile(MutationProfile::Full, 10_000, "result = first + second\n").candidates,
    );
}

#[test]
fn omits_candidates_for_unselected_operators() {
    let mut operators = MutationOperatorSelection::default();
    operators.exclude(MutationOperator::BinaryAddSub);
    let output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        "result = left + right\n",
    );
    assert!(output.candidates.is_empty());
}

const MUTABLE_OPERATOR_TOKENS: &[&str] = &[
    "==", "!=", "<", "<=", ">", ">=", "in", "not in", "is", "is not", "and", "or", "+=", "-=", "*",
    "/", "//", "%", "&", "|", "<<", ">>", "break", "continue", "True", "False", "+", "-", "not",
];

const NESTED_QUOTE_PAIRS: &[(&str, &str, &str)] = &[
    (
        "single-quoted",
        "label = 'plain \"double\" text'\n",
        "label = 'mutable + \"double\" text'\n",
    ),
    (
        "double-quoted",
        "label = \"plain 'single' text\"\n",
        "label = \"mutable == 'single' text\"\n",
    ),
    (
        "triple-single-quoted",
        "label = '''plain \"double\" and 'single' text'''\n",
        "label = '''mutable True + \"double\" and 'single' text'''\n",
    ),
    (
        "triple-double-quoted",
        "label = \"\"\"plain 'single' and \"double\" text\"\"\"\n",
        "label = \"\"\"mutable False - 'single' and \"double\" text\"\"\"\n",
    ),
];

fn assert_only_trailing_expression_changes(literal_line: &str) {
    let source = format!("{literal_line}result = left + right\n");
    let output = analyze(&source);
    assert!(output.diagnostics.is_empty());
    assert_eq!(output.candidates.len(), 1);
    let candidate = &output.candidates[0];
    assert_eq!(candidate.original, "+");
    assert_eq!(candidate.replacement, "-");
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    assert_eq!(&source[..start], format!("{literal_line}result = left "));
    assert_eq!(&source[end..], " right\n");
    let mut mutated = source.clone();
    mutated.replace_range(start..end, &candidate.replacement);
    assert_eq!(mutated, format!("{literal_line}result = left - right\n"));
}

#[test]
fn ordinary_string_literals_ignore_every_mutable_operator_token() {
    assert!(
        analyze("label = 'ordinary operator-free text'\n")
            .candidates
            .is_empty()
    );
    for token in MUTABLE_OPERATOR_TOKENS {
        let source = format!("label = {token:?}\n");
        assert!(
            analyze(&source).candidates.is_empty(),
            "ordinary string content {token:?} produced a candidate"
        );
        assert_only_trailing_expression_changes(&source);
    }
}

#[test]
fn nested_quote_pairs_preserve_every_surrounding_string_byte() {
    for (name, benign, adversarial) in NESTED_QUOTE_PAIRS {
        for source in [benign, adversarial] {
            assert!(
                analyze(source).candidates.is_empty(),
                "nested quote fixture {name} produced a literal-content candidate"
            );
            assert_only_trailing_expression_changes(source);
        }
    }
}

#[test]
fn interpolated_string_pairs_skip_literals_and_retain_expressions() {
    for (flavor, literal, expression) in [
        (
            "f-string",
            "value = f\"literal == + True {plain}\"\n",
            "value = f\"literal == + True {left + right} {enabled is not None}\"\n",
        ),
        (
            "t-string",
            "value = t\"literal == + True {plain}\"\n",
            "value = t\"literal == + True {left + right} {enabled is not None}\"\n",
        ),
    ] {
        assert!(
            analyze(literal).candidates.is_empty(),
            "{flavor} literal content produced a candidate"
        );
        let observed = analyze(expression)
            .candidates
            .into_iter()
            .map(|candidate| {
                (
                    candidate.original,
                    candidate.replacement,
                    candidate.operator,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            [
                ("+".to_owned(), "-".to_owned(), "binary_add_sub".to_owned()),
                ("is not".to_owned(), "is".to_owned(), "identity".to_owned()),
            ],
            "{flavor} interpolation expressions were not analyzed exactly"
        );
    }
}

#[test]
fn pep_695_type_positions_suppress_runtime_mutations() {
    let source = concat!(
        "def convert[T: Left | Right = list[str]](value: T):\n",
        "    return left | right\n",
        "class Box[U: Base | None = tuple[int]]:\n",
        "    runtime = first + second\n",
    );
    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "bitwise_and_or" | "binary_add_sub"
            )
        })
        .map(|candidate| (candidate.original.as_str(), candidate.line))
        .collect::<Vec<_>>();

    assert_eq!(observed, [("|", 2), ("+", 4)]);
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn pep_695_type_positions_emit_type_candidates() {
    let source = concat!(
        "from typing import Sequence\n",
        "type Values[T: list[str] = list[bytes], *Ts = list[float], **P = list[bool]] = list[int]\n",
        "class Outer:\n",
        "    type Nested[U: list[int]] = list[bytes]\n",
    );
    let output = analyze_types(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        observed,
        [
            ("list[str]", "Sequence[str]", 2, Some("Values")),
            ("list[bytes]", "Sequence[bytes]", 2, Some("Values")),
            ("list[float]", "Sequence[float]", 2, Some("Values")),
            ("list[bool]", "Sequence[bool]", 2, Some("Values")),
            ("list[int]", "Sequence[int]", 2, Some("Values")),
            ("list[int]", "Sequence[int]", 4, Some("Outer.Nested")),
            ("list[bytes]", "Sequence[bytes]", 4, Some("Outer.Nested")),
        ]
    );
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn pep_695_type_parameters_shadow_imported_replacement_spellings() {
    let source = concat!(
        "from typing import Sequence\n",
        "def convert[Sequence](value: list[int]) -> list[str]:\n",
        "    local: list[float]\n",
        "class Box[Sequence]:\n",
        "    field: list[bytes]\n",
        "type Alias[Sequence] = list[int]\n",
        "safe: list[bool]\n",
    );
    let output = analyze_types(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(observed, [("list[bool]", "Sequence[bool]", 7, None)]);
}

#[test]
fn type_annotations_emit_supported_candidates_in_source_order() {
    let source = "from typing import AbstractSet, Iterator, Mapping, Optional\nimport typing as t\nfrom collections.abc import Iterable, Sequence\n\nmodule_value: Optional[int]\n\nclass Model:\n    names: list[str]\n\n    def convert(self, name: str | None, age: t.Optional[int]) -> set[str]:\n        mapping: dict[str, int] = {}\n        values: Iterable[str] = []\n        ordered: Sequence[str] = []\n        return set()\n";
    let output = analyze_types(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.starts_with("type_"))
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();
    assert_eq!(
        candidates,
        vec![
            ("Optional[int]", "int", "type_nullable_remove"),
            ("list[str]", "Sequence[str]", "type_list_sequence"),
            ("list[str]", "list[str] | None", "type_nullable_add"),
            ("str | None", "str", "type_nullable_remove"),
            ("t.Optional[int]", "int", "type_nullable_remove"),
            ("set[str]", "set[str] | None", "type_nullable_add"),
            ("set[str]", "AbstractSet[str]", "type_set_abstract_set"),
            ("dict[str, int]", "Mapping[str, int]", "type_dict_mapping"),
            (
                "dict[str, int]",
                "dict[str, int] | None",
                "type_nullable_add"
            ),
            ("Iterable[str]", "Iterator[str]", "type_iterable_iterator"),
            ("Iterable[str]", "Iterable[str] | None", "type_nullable_add"),
            ("Sequence[str]", "list[str]", "type_list_sequence"),
            ("Sequence[str]", "Sequence[str] | None", "type_nullable_add"),
            ("Sequence[str]", "Iterable[str]", "type_sequence_iterable"),
        ]
    );
}

#[test]
fn typing_import_rebinding_linear() {
    for (name, source, expected) in [
        (
            "unaliased import assignment and restoration",
            "from typing import Sequence\nbefore: list[str]\nSequence = local_sequence\nafter: list[str]\nfrom typing import Sequence as Sequence\nrestored: list[str]\n",
            vec![
                (36, 9, 2, None, "Sequence[str]"),
                (139, 9, 6, None, "Sequence[str]"),
            ],
        ),
        (
            "direct alias assignment and restoration",
            "from typing import Sequence as Seq\nbefore: list[str]\nSeq = local_sequence\nafter: list[str]\nfrom typing import Sequence as Seq\nrestored: list[str]\n",
            vec![(43, 9, 2, None, "Seq[str]"), (136, 9, 6, None, "Seq[str]")],
        ),
        (
            "delete rebinding",
            "from typing import Sequence\nbefore: list[str]\ndel Sequence\nafter: list[str]\n",
            vec![(36, 9, 2, None, "Sequence[str]")],
        ),
        (
            "function definition rebinding and restoration",
            "from typing import Sequence\nbefore: list[str]\ndef Sequence():\n    pass\nafter: list[str]\nfrom typing import Sequence\nrestored: list[str]\n",
            vec![
                (36, 9, 2, None, "Sequence[str]"),
                (126, 9, 7, None, "Sequence[str]"),
            ],
        ),
        (
            "class definition rebinding and restoration",
            "from typing import Sequence\nbefore: list[str]\nclass Sequence:\n    pass\nafter: list[str]\nfrom typing import Sequence\nrestored: list[str]\n",
            vec![
                (36, 9, 2, None, "Sequence[str]"),
                (126, 9, 7, None, "Sequence[str]"),
            ],
        ),
    ] {
        let output = analyze_types(source);
        let actual: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_list_sequence")
            .map(|candidate| {
                (
                    candidate.span.start,
                    candidate.span.length,
                    candidate.line,
                    candidate.symbol.as_deref(),
                    candidate.replacement.as_str(),
                )
            })
            .collect();
        assert_eq!(actual, expected, "{name}");
        for candidate in &output.candidates {
            apply_candidate_and_reparse(source, candidate);
        }
    }
}

#[test]
fn typing_module_alias_rebinding_linear() {
    for (name, source, expected) in [
        (
            "module alias assignment and restoration",
            "import typing as t\nbefore: list[str]\nt = local_typing\nafter: list[str]\nimport typing as t\nrestored: list[str]\n",
            vec![
                (27, 9, 2, None, "t.Sequence[str]"),
                (100, 9, 6, None, "t.Sequence[str]"),
            ],
        ),
        (
            "unsupported competing module import and restoration",
            "import typing as t\nbefore: list[str]\nimport local as t\nafter: list[str]\nimport typing as t\nrestored: list[str]\n",
            vec![
                (27, 9, 2, None, "t.Sequence[str]"),
                (101, 9, 6, None, "t.Sequence[str]"),
            ],
        ),
        (
            "unsupported wildcard import invalidates module aliases",
            "import typing as t\nbefore: list[str]\nfrom local import *\nafter: list[str]\n",
            vec![(27, 9, 2, None, "t.Sequence[str]")],
        ),
    ] {
        let output = analyze_types(source);
        let actual: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_list_sequence")
            .map(|candidate| {
                (
                    candidate.span.start,
                    candidate.span.length,
                    candidate.line,
                    candidate.symbol.as_deref(),
                    candidate.replacement.as_str(),
                )
            })
            .collect();
        assert_eq!(actual, expected, "{name}");
        for candidate in &output.candidates {
            apply_candidate_and_reparse(source, candidate);
        }
    }
}

const TYPING_IMPORT_REBINDING_SCOPE_SOURCE: &str = concat!(
    "from typing import Sequence\n",
    "\n",
    "def Sequence(value: Sequence[str]) -> Sequence[str]:\n",
    "    pass\n",
    "same_name_after: list[str]\n",
    "from typing import Sequence\n",
    "\n",
    "def parameter_shadow(Sequence):\n",
    "    hidden: list[str]\n",
    "\n",
    "def local_shadow():\n",
    "    hidden_before: list[str]\n",
    "    Sequence = local_sequence\n",
    "    from typing import Sequence\n",
    "    restored: list[str]\n",
    "\n",
    "def nested_outer():\n",
    "    visible: list[str]\n",
    "    def nested():\n",
    "        hidden: list[str]\n",
    "        Sequence = local_sequence\n",
    "    class Nested:\n",
    "        visible: list[str]\n",
    "        Sequence = local_sequence\n",
    "        hidden: list[str]\n",
    "    visible_after: list[str]\n",
    "\n",
    "class Container:\n",
    "    Sequence = local_sequence\n",
    "    method_field: list[str]\n",
    "    def method(self, value: list[str]):\n",
    "        visible_body: list[str]\n",
    "\n",
    "def global_binding():\n",
    "    global Sequence\n",
    "    visible_before: list[str]\n",
    "    Sequence = local_sequence\n",
    "    hidden_after: list[str]\n",
    "\n",
    "def nonlocal_outer():\n",
    "    from typing import Sequence\n",
    "    def inner():\n",
    "        nonlocal Sequence\n",
    "        visible_before: list[str]\n",
    "        Sequence = local_sequence\n",
    "        hidden_after: list[str]\n",
    "    visible_outer: list[str]\n",
    "\n",
    "def with_target(manager):\n",
    "    hidden_before: list[str]\n",
    "    with manager as Sequence:\n",
    "        hidden_inside: list[str]\n",
    "\n",
    "def except_target():\n",
    "    hidden_before: list[str]\n",
    "    try:\n",
    "        work()\n",
    "    except Error as Sequence:\n",
    "        hidden_inside: list[str]\n",
    "\n",
    "def pattern_target(value):\n",
    "    hidden_before: list[str]\n",
    "    match value:\n",
    "        case {\"item\": Sequence}:\n",
    "            hidden_inside: list[str]\n",
    "\n",
    "def named_target(value):\n",
    "    hidden_before: list[str]\n",
    "    if (Sequence := value):\n",
    "        hidden_inside: list[str]\n",
    "\n",
    "def comprehension_target(values):\n",
    "    visible_before: list[str]\n",
    "    result = [item for Sequence in values for item in Sequence]\n",
    "    visible_after: list[str]\n",
    "\n",
    "untouched: list[str]\n",
);

#[test]
#[allow(clippy::too_many_lines)]
fn typing_import_rebinding_scope() {
    let source = TYPING_IMPORT_REBINDING_SCOPE_SOURCE;
    let output = analyze_types(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.span.length,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.original.as_str(),
                candidate.replacement.as_str(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        vec![
            (49, 13, 3, Some("Sequence"), "Sequence[str]", "list[str]"),
            (67, 13, 3, Some("Sequence"), "Sequence[str]", "list[str]"),
            (
                327,
                9,
                15,
                Some("local_shadow"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                371,
                9,
                18,
                Some("nested_outer"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                494,
                9,
                23,
                Some("nested_outer.Nested"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                583,
                9,
                26,
                Some("nested_outer"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                731,
                9,
                32,
                Some("Container.method"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                804,
                9,
                36,
                Some("global_binding"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                994,
                9,
                44,
                Some("nonlocal_outer.inner"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                1089,
                9,
                47,
                Some("nonlocal_outer"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                1671,
                9,
                73,
                Some("comprehension_target"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                1764,
                9,
                75,
                Some("comprehension_target"),
                "list[str]",
                "Sequence[str]"
            ),
            (1786, 9, 77, None, "list[str]", "Sequence[str]"),
        ]
    );
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }

    let alias_source = concat!(
        "import typing as t\n",
        "def module_alias_shadow():\n",
        "    hidden_before: list[str]\n",
        "    t = local_typing\n",
        "    import typing as t\n",
        "    restored: list[str]\n",
        "untouched: list[str]\n",
    );
    let alias_output = analyze_types(alias_source);
    let alias_actual: Vec<_> = alias_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.span.length,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.replacement.as_str(),
            )
        })
        .collect();
    assert_eq!(
        alias_actual,
        vec![
            (133, 9, 6, Some("module_alias_shadow"), "t.Sequence[str]"),
            (154, 9, 7, None, "t.Sequence[str]"),
        ]
    );
    for candidate in &alias_output.candidates {
        apply_candidate_and_reparse(alias_source, candidate);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn typing_import_rebinding_control_flow() {
    for (name, source, expected) in [
        (
            "if joins optional and identical branches",
            concat!(
                "from typing import Sequence\n",
                "before: list[str]\n",
                "if condition:\n",
                "    Sequence = local_sequence\n",
                "after_optional: list[str]\n",
                "Sequence = local_sequence\n",
                "if condition:\n",
                "    from typing import Sequence\n",
                "else:\n",
                "    from typing import Sequence\n",
                "after_identical: list[str]\n",
            ),
            vec![
                (36, 2, None, "Sequence[str]"),
                (243, 11, None, "Sequence[str]"),
            ],
        ),
        (
            "returning branch is not a continuation exit",
            concat!(
                "from typing import Sequence\n",
                "def reachable(flag):\n",
                "    from typing import Sequence\n",
                "    if flag:\n",
                "        Sequence = local_sequence\n",
                "        return\n",
                "    after_return: list[str]\n",
                "untouched: list[str]\n",
            ),
            vec![
                (161, 7, Some("reachable"), "Sequence[str]"),
                (182, 8, None, "Sequence[str]"),
            ],
        ),
        (
            "loops include their zero iteration exits",
            concat!(
                "Sequence = local_sequence\n",
                "for item in items:\n",
                "    from typing import Sequence\n",
                "    inside_for: list[str]\n",
                "after_for: list[str]\n",
                "from typing import Sequence\n",
                "while condition:\n",
                "    Sequence = local_sequence\n",
                "after_while: list[str]\n",
                "from typing import Sequence\n",
                "untouched: list[str]\n",
            ),
            vec![
                (93, 4, None, "Sequence[str]"),
                (261, 11, None, "Sequence[str]"),
            ],
        ),
        (
            "try joins normal handlers else and applies finally",
            concat!(
                "from typing import Sequence\n",
                "try:\n",
                "    Sequence = local_sequence\n",
                "except Error:\n",
                "    from typing import Sequence\n",
                "after_ambiguous: list[str]\n",
                "Sequence = local_sequence\n",
                "try:\n",
                "    from typing import Sequence\n",
                "except Error:\n",
                "    from typing import Sequence\n",
                "else:\n",
                "    from typing import Sequence\n",
                "after_identical: list[str]\n",
                "Sequence = local_sequence\n",
                "try:\n",
                "    Sequence = local_sequence\n",
                "except Error:\n",
                "    Sequence = local_sequence\n",
                "finally:\n",
                "    from typing import Sequence\n",
                "after_finally: list[str]\n",
            ),
            vec![
                (300, 14, None, "Sequence[str]"),
                (471, 22, None, "Sequence[str]"),
            ],
        ),
        (
            "with and except targets bind before their suites",
            concat!(
                "from typing import Sequence\n",
                "with manager as Sequence:\n",
                "    hidden_with: list[str]\n",
                "after_with: list[str]\n",
                "from typing import Sequence\n",
                "try:\n",
                "    work()\n",
                "except Error as Sequence:\n",
                "    hidden_except: list[str]\n",
                "after_except: list[str]\n",
                "from typing import Sequence\n",
                "untouched: list[str]\n",
            ),
            vec![(265, 12, None, "Sequence[str]")],
        ),
        (
            "match includes unmatched flow unless irrefutable",
            concat!(
                "from typing import Sequence\n",
                "match value:\n",
                "    case {\"item\": Sequence}:\n",
                "        hidden_case: list[str]\n",
                "after_optional: list[str]\n",
                "Sequence = local_sequence\n",
                "match value:\n",
                "    case _:\n",
                "        from typing import Sequence\n",
                "after_irrefutable: list[str]\n",
                "from typing import Sequence\n",
                "match value:\n",
                "    case Sequence if guard:\n",
                "        from typing import Sequence\n",
                "after_guarded: list[str]\n",
                "from typing import Sequence\n",
                "untouched: list[str]\n",
            ),
            vec![
                (233, 10, None, "Sequence[str]"),
                (412, 17, None, "Sequence[str]"),
            ],
        ),
        (
            "named expression binding precedes branch suites",
            concat!(
                "from typing import Sequence\n",
                "if (Sequence := local_sequence):\n",
                "    hidden_named: list[str]\n",
                "after_named: list[str]\n",
                "from typing import Sequence\n",
                "untouched: list[str]\n",
            ),
            vec![(151, 6, None, "Sequence[str]")],
        ),
    ] {
        let output = analyze_types(source);
        let actual: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_list_sequence")
            .map(|candidate| {
                (
                    candidate.span.start,
                    candidate.line,
                    candidate.symbol.as_deref(),
                    candidate.replacement.as_str(),
                )
            })
            .collect();
        assert_eq!(actual, expected, "{name}");
        for candidate in &output.candidates {
            apply_candidate_and_reparse(source, candidate);
        }
    }
}

#[test]
fn typing_import_rebinding_finally_preserves_exit_categories() {
    let source = concat!(
        "from typing import Sequence\n",
        "\n",
        "def return_path(flag):\n",
        "    from typing import Sequence\n",
        "    try:\n",
        "        if flag:\n",
        "            Sequence = local_sequence\n",
        "            return\n",
        "    finally:\n",
        "        in_return_finally: list[str]\n",
        "    after_return: list[str]\n",
        "\n",
        "def raise_path(flag):\n",
        "    from typing import Sequence\n",
        "    try:\n",
        "        if flag:\n",
        "            Sequence = local_sequence\n",
        "            raise Error\n",
        "    finally:\n",
        "        in_raise_finally: list[str]\n",
        "    after_raise: list[str]\n",
        "\n",
        "def break_path(flag, items):\n",
        "    from typing import Sequence\n",
        "    for item in items:\n",
        "        try:\n",
        "            if flag:\n",
        "                Sequence = local_sequence\n",
        "                break\n",
        "        finally:\n",
        "            in_break_finally: list[str]\n",
        "        after_break: list[str]\n",
        "    after_break_loop: list[str]\n",
        "\n",
        "def continue_path(flag, items):\n",
        "    from typing import Sequence\n",
        "    for item in items:\n",
        "        try:\n",
        "            if flag:\n",
        "                Sequence = local_sequence\n",
        "                continue\n",
        "        finally:\n",
        "            in_continue_finally: list[str]\n",
        "    after_continue_loop: list[str]\n",
        "\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (235, 9, 11, Some("return_path"), "Sequence[str]"),
            (454, 9, 21, Some("raise_path"), "Sequence[str]"),
            (725, 9, 32, Some("break_path"), "Sequence[str]"),
            (1063, 9, 46, None, "Sequence[str]"),
        ],
    );
}

#[test]
fn typing_import_rebinding_match_propagates_failed_case_bindings() {
    let source = concat!(
        "from typing import Sequence\n",
        "match value:\n",
        "    case 0 if (Sequence := local_sequence):\n",
        "        pass\n",
        "    case _:\n",
        "        after_guard_failure: list[str]\n",
        "after_guard_match: list[str]\n",
        "from typing import Sequence\n",
        "match value:\n",
        "    case [Sequence, 0]:\n",
        "        pass\n",
        "    case _:\n",
        "        after_pattern_failure: list[str]\n",
        "after_pattern_match: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(379, 9, 16, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_loop_heads_include_back_edges() {
    let source = concat!(
        "from typing import Sequence\n",
        "\n",
        "def for_back_edge(items):\n",
        "    from typing import Sequence\n",
        "    for item in items:\n",
        "        before_rebind: list[str]\n",
        "        Sequence = local_sequence\n",
        "    after_for: list[str]\n",
        "\n",
        "def while_continue(condition, flag):\n",
        "    from typing import Sequence\n",
        "    while condition:\n",
        "        before_continue: list[str]\n",
        "        if flag:\n",
        "            Sequence = local_sequence\n",
        "            continue\n",
        "        break\n",
        "    after_while: list[str]\n",
        "\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(457, 9, 20, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_definition_header_function_defaults() {
    let source = concat!(
        "from typing import Sequence\n",
        "def default_binding(\n",
        "    value: list[str] = (Sequence := local_sequence),\n",
        "    other: list[str] = None,\n",
        ") -> list[str]:\n",
        "    pass\n",
        "after_default: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(220, 9, 9, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_definition_header_function_decorators() {
    let source = concat!(
        "from typing import Sequence\n",
        "@(Sequence := decorator)\n",
        "def decorated(value: list[str]) -> list[str]:\n",
        "    pass\n",
        "after_decorator: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(174, 9, 7, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_definition_header_class_expressions() {
    let source = concat!(
        "from typing import Sequence\n",
        "@(Sequence := decorator)\n",
        "class Decorated:\n",
        "    in_decorated: list[str]\n",
        "after_decorated: list[str]\n",
        "from typing import Sequence\n",
        "class Based((Sequence := Base)):\n",
        "    in_based: list[str]\n",
        "after_based: list[str]\n",
        "from typing import Sequence\n",
        "class Keyword(metaclass=(Sequence := Meta)):\n",
        "    in_keyword: list[str]\n",
        "after_keyword: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(396, 9, 15, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_definition_header_lambda_defaults_only() {
    let source = concat!(
        "from typing import Sequence\n",
        "with_default = lambda value=(Sequence := local_sequence): value\n",
        "after_default: list[str]\n",
        "from typing import Sequence\n",
        "body_only = lambda: (Sequence := local_sequence)\n",
        "after_body: list[str]\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (206, 9, 6, None, "Sequence[str]"),
            (227, 9, 7, None, "Sequence[str]"),
        ],
    );
}

#[test]
fn typing_import_rebinding_annotated_assignment_execution_order() {
    let source = concat!(
        "from typing import Sequence\n",
        "Sequence: list[str] = object\n",
        "after_module_valued: list[str]\n",
        "from typing import Sequence\n",
        "Sequence: object\n",
        "after_module_valueless: list[str]\n",
        "from typing import Sequence\n",
        "value: list[str] = (Sequence := object)\n",
        "after_module_rhs: list[str]\n",
        "from typing import Sequence\n",
        "class ClassValued:\n",
        "    Sequence: list[str] = object\n",
        "    after_valued: list[str]\n",
        "class ClassValueless:\n",
        "    Sequence: object\n",
        "    after_valueless: list[str]\n",
        "class ClassRhs:\n",
        "    value: list[str] = (Sequence := object)\n",
        "    after_rhs: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (157, 9, 6, None, "Sequence[str]"),
            (435, 9, 16, Some("ClassValueless"), "Sequence[str]"),
            (569, 9, 21, None, "Sequence[str]"),
        ],
    );
}

#[test]
fn typing_import_rebinding_class_external_writes_project_to_target_scope() {
    let source = concat!(
        "from typing import Sequence\n",
        "class DirectWrite:\n",
        "    global Sequence\n",
        "    Sequence = object\n",
        "    def method(self):\n",
        "        hidden: list[str]\n",
        "after_direct_write: list[str]\n",
        "class DirectReimport:\n",
        "    global Sequence\n",
        "    from typing import Sequence\n",
        "    def method(self):\n",
        "        restored: list[str]\n",
        "after_direct_reimport: list[str]\n",
        "Sequence = object\n",
        "import typing as t\n",
        "class AliasWrite:\n",
        "    global t\n",
        "    t = object\n",
        "    def method(self):\n",
        "        hidden: list[str]\n",
        "after_alias_write: list[str]\n",
        "class AliasReimport:\n",
        "    global t\n",
        "    import typing as t\n",
        "    def method(self):\n",
        "        restored: list[str]\n",
        "after_alias_reimport: list[str]\n",
        "t = object\n",
        "def direct_outer():\n",
        "    from typing import Sequence\n",
        "    class DirectWrite:\n",
        "        nonlocal Sequence\n",
        "        Sequence = object\n",
        "        def method(self):\n",
        "            hidden: list[str]\n",
        "    after_write: list[str]\n",
        "    class DirectReimport:\n",
        "        nonlocal Sequence\n",
        "        from typing import Sequence\n",
        "        def method(self):\n",
        "            restored: list[str]\n",
        "    after_reimport: list[str]\n",
        "def alias_outer():\n",
        "    import typing as t\n",
        "    class AliasWrite:\n",
        "        nonlocal t\n",
        "        t = object\n",
        "        def method(self):\n",
        "            hidden: list[str]\n",
        "    after_write: list[str]\n",
        "    class AliasReimport:\n",
        "        nonlocal t\n",
        "        import typing as t\n",
        "        def method(self):\n",
        "            restored: list[str]\n",
        "    after_reimport: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (281, 9, 12, Some("DirectReimport.method"), "Sequence[str]"),
            (314, 9, 13, None, "Sequence[str]"),
            (581, 9, 26, Some("AliasReimport.method"), "t.Sequence[str]"),
            (613, 9, 27, None, "t.Sequence[str]"),
            (
                980,
                9,
                41,
                Some("direct_outer.DirectReimport.method"),
                "Sequence[str]",
            ),
            (1010, 9, 42, Some("direct_outer"), "Sequence[str]"),
            (
                1324,
                9,
                55,
                Some("alias_outer.AliasReimport.method"),
                "t.Sequence[str]",
            ),
            (1354, 9, 56, Some("alias_outer"), "t.Sequence[str]"),
            (1403, 9, 58, None, "Sequence[str]"),
        ],
    );
}

#[test]
fn typing_import_rebinding_nested_class_globals_preserve_function_fallback() {
    let source = concat!(
        "Sequence = object\n",
        "t = object\n",
        "def direct_outer():\n",
        "    from typing import Sequence\n",
        "    class GlobalWrite:\n",
        "        global Sequence\n",
        "        Sequence = object\n",
        "        def method(self):\n",
        "            preserved: list[str]\n",
        "    after_class: list[str]\n",
        "def alias_outer():\n",
        "    import typing as t\n",
        "    class GlobalWrite:\n",
        "        global t\n",
        "        t = object\n",
        "        def method(self):\n",
        "            preserved: list[str]\n",
        "    after_class: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (
                203,
                9,
                9,
                Some("direct_outer.GlobalWrite.method"),
                "Sequence[str]",
            ),
            (230, 9, 10, Some("direct_outer"), "Sequence[str]"),
            (
                390,
                9,
                17,
                Some("alias_outer.GlobalWrite.method"),
                "t.Sequence[str]",
            ),
            (417, 9, 18, Some("alias_outer"), "t.Sequence[str]"),
            (466, 9, 20, None, "Sequence[str]"),
        ],
    );
}

#[test]
fn typing_import_rebinding_try_handler_includes_unknown_wildcard_effect() {
    let source = concat!(
        "from typing import Sequence\n",
        "try:\n",
        "    from unknown import *\n",
        "    raise Error\n",
        "except Error:\n",
        "    hidden_direct: list[str]\n",
        "Sequence = object\n",
        "import typing as t\n",
        "try:\n",
        "    from unknown import *\n",
        "    raise Error\n",
        "except Error:\n",
        "    hidden_alias: list[str]\n",
        "t = object\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(294, 9, 16, None, "Sequence[str]")]);
}

#[test]
fn type_annotations_ignore_quoted_and_unrecognized_forms() {
    let source = "from typing import Annotated, Any, Callable, Optional, TypeVar\nfrom local import Optional as LocalOptional\n\nT = TypeVar('T')\nclass Sequence: pass\nquoted: 'Optional[int]'\nannotated: Annotated[list[str], 'meta']\nany_value: Any\ncallback: Callable[[str], int]\ngeneric: T\nuser_sequence: Sequence[str]\nlocal_optional: LocalOptional[int]\n";
    let output = analyze_types(source);
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.starts_with("type_"))
    );
}

#[test]
fn type_annotations_reject_disallowed_nested_types_and_object_nullable_addition() {
    let source = "from typing import Annotated, Any, Callable, Literal, Optional, Protocol, TypeVar\nimport typing as t\n\nT = TypeVar('T')\nAlias = str\nobject_value: object\noptional_object: Optional[object]\nobject_union: object | None\nobject_list: list[object]\noptional_any: Optional[Any]\ncallable_value: Callable[[str], int] | None\noptional_type_var: Optional[T]\naliased: Alias | None\nitems: list[Any]\nannotated: Optional[Annotated[list[str], 'meta']]\nliteral: Literal[\"x\"] | None\nqualified_literal: t.Literal[\"x\"] | None\nprotocol: Protocol | None\nqualified_protocol: t.Protocol | None\n";
    let output = analyze_types(source);
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.starts_with("type_"))
    );
}

#[test]
fn type_annotations_resolve_unaliased_and_aliased_collections_abc_modules() {
    let source = "import collections.abc\nimport collections.abc as cabc\n\nfirst: collections.abc.Iterable[str]\nsecond: collections.abc.Sequence[str]\nthird: cabc.Iterable[str]\nfourth: cabc.Sequence[str]\n";
    let output = analyze_types(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.operator.starts_with("type_") && candidate.operator != "type_list_sequence"
        })
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();
    assert_eq!(
        candidates,
        vec![
            (
                "collections.abc.Iterable[str]",
                "collections.abc.Iterator[str]",
                "type_iterable_iterator"
            ),
            (
                "collections.abc.Iterable[str]",
                "collections.abc.Iterable[str] | None",
                "type_nullable_add"
            ),
            (
                "collections.abc.Sequence[str]",
                "collections.abc.Sequence[str] | None",
                "type_nullable_add"
            ),
            (
                "collections.abc.Sequence[str]",
                "collections.abc.Iterable[str]",
                "type_sequence_iterable"
            ),
            (
                "cabc.Iterable[str]",
                "cabc.Iterator[str]",
                "type_iterable_iterator"
            ),
            (
                "cabc.Iterable[str]",
                "cabc.Iterable[str] | None",
                "type_nullable_add"
            ),
            (
                "cabc.Sequence[str]",
                "cabc.Sequence[str] | None",
                "type_nullable_add"
            ),
            (
                "cabc.Sequence[str]",
                "cabc.Iterable[str]",
                "type_sequence_iterable"
            ),
        ]
    );
}
#[test]
fn type_annotations_emit_reverse_collection_and_iterable_mutations() {
    let source = "from typing import AbstractSet, Iterable, Iterator, Mapping, Sequence\n\nforward_list: list[str]\nreverse_sequence: Sequence[str]\nforward_set: set[str]\nreverse_set: AbstractSet[str]\nforward_dict: dict[str, int]\nreverse_mapping: Mapping[str, int]\nforward_iterable: Iterable[str]\nreverse_iterator: Iterator[str]\n";
    let output = analyze_types(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "type_list_sequence"
                    | "type_set_abstract_set"
                    | "type_dict_mapping"
                    | "type_iterable_iterator"
                    | "type_sequence_iterable"
            )
        })
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();
    assert_eq!(
        candidates,
        vec![
            ("list[str]", "Sequence[str]", "type_list_sequence"),
            ("Sequence[str]", "list[str]", "type_list_sequence"),
            ("Sequence[str]", "Iterable[str]", "type_sequence_iterable"),
            ("set[str]", "AbstractSet[str]", "type_set_abstract_set"),
            ("AbstractSet[str]", "set[str]", "type_set_abstract_set"),
            ("dict[str, int]", "Mapping[str, int]", "type_dict_mapping"),
            ("Mapping[str, int]", "dict[str, int]", "type_dict_mapping"),
            ("Iterable[str]", "Iterator[str]", "type_iterable_iterator"),
            ("Iterator[str]", "Iterable[str]", "type_iterable_iterator"),
        ]
    );
}

#[test]
fn type_annotations_require_resolvable_abstract_destinations() {
    let missing = analyze_types("value: list[str]\n");
    assert!(
        missing
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "type_list_sequence")
    );
    let direct = analyze_types("from typing import Sequence as Seq\nvalue: list[str]\n");
    assert!(direct.candidates.iter().any(|candidate| (
        candidate.original.as_str(),
        candidate.replacement.as_str(),
        candidate.operator.as_str()
    ) == (
        "list[str]",
        "Seq[str]",
        "type_list_sequence"
    )));
    let qualified = analyze_types("import typing as t\nvalue: list[str]\n");
    assert!(qualified.candidates.iter().any(|candidate| (
        candidate.original.as_str(),
        candidate.replacement.as_str(),
        candidate.operator.as_str()
    ) == (
        "list[str]",
        "t.Sequence[str]",
        "type_list_sequence"
    )));
}
#[test]
fn type_annotations_respect_line_and_symbol_filters() {
    let source = "from typing import AbstractSet, Mapping, Sequence\n\nclass Model:\n    field: list[str]\n\ndef convert(value: str) -> set[str]:\n    local: dict[str, int] = {}\n    return set()\n";
    let mut operators = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::TypeNullableAdd,
        MutationOperator::TypeListSequence,
        MutationOperator::TypeSetAbstractSet,
        MutationOperator::TypeMapping,
    ] {
        operators.include(operator);
    }
    let line_output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[LineRange { start: 4, end: 4 }],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    assert_eq!(
        line_output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.starts_with("type_"))
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec!["list[str]", "list[str]"]
    );
    let symbol_output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &["pkg.sample:convert".to_owned()],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    assert_eq!(
        symbol_output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.starts_with("type_"))
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec![
            "str",
            "set[str]",
            "set[str]",
            "dict[str, int]",
            "dict[str, int]"
        ]
    );
}
#[test]
fn grammar_tokens_do_not_emit_expression_operator_mutations() {
    let source = concat!(
        "from package import *\n",
        "for item in items:\n    pass\n",
        "values = [item for item in items]\n",
        "result = call(*args, **kwargs)\n",
        "match value:\n    case left | right:\n        pass\n",
        "member = item in items\n",
        "product = left * right\n",
        "union = left | right\n",
        "nested_membership = [item for item in items] == values\n",
        "nested_unpacking = call(*args) * value\n",
    );
    let output = analyze(source);
    assert!(!output.candidates.iter().any(|candidate| {
        candidate.line <= 8
            && matches!(
                candidate.operator.as_str(),
                "membership" | "binary_mul_div" | "bitwise_and_or"
            )
    }));
    assert!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 9 && candidate.operator == "membership")
    );
    assert!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 10 && candidate.operator == "binary_mul_div")
    );
    assert!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 11 && candidate.operator == "bitwise_and_or")
    );
    assert!(
        !output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 12 && candidate.operator == "membership")
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| { candidate.line == 13 && candidate.operator == "binary_mul_div" })
            .count(),
        1
    );
}

proptest! {
    #[test]
    fn arbitrary_python_input_has_ordered_in_bounds_candidates(source in ".{0,4096}") {
        let output = analyze(&source);
        let mut previous_start = 0;
        for candidate in output.candidates {
            let start = usize::try_from(candidate.span.start).expect("span start fits usize");
            let length = usize::try_from(candidate.span.length).expect("span length fits usize");
            let end = start.checked_add(length).expect("candidate span does not overflow");
            prop_assert!(previous_start <= start);
            prop_assert!(end <= source.len());
            prop_assert_eq!(&source[start..end], candidate.original);
            previous_start = start;
        }
    }
}

#[test]
fn emits_the_mvp_operator_replacements_in_source_order() {
    let source = "def f(a, b, xs, flag):\n    value = a == b and a not in xs and a is not b\n    value += a * b // 2 % 2\n    return not flag, +a, -b, True, False\n";
    let output = analyze(source);
    let pairs: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("==", "!=", "compare_eq_ne"),
            ("and", "or", "boolean_and_or"),
            ("not in", "in", "membership"),
            ("and", "or", "boolean_and_or"),
            ("is not", "is", "identity"),
            ("+=", "-=", "augmented_add_sub"),
            ("*", "/", "binary_mul_div"),
            ("//", "%", "binary_floor_mod"),
            ("%", "//", "binary_floor_mod"),
            (
                "not flag, +a, -b, True, False",
                "[not flag, +a, -b, True, False]",
                "collection_list_tuple"
            ),
            ("not flag", "flag", "remove_not"),
            ("+", "-", "unary_sign"),
            ("-", "+", "unary_sign"),
            ("True", "False", "boolean_literal"),
            ("False", "True", "boolean_literal")
        ]
    );
}

#[test]
fn emits_complete_candidate_records_in_source_order() {
    let source =
        "top = left == right\ndef decide(flag, value):\n    return not flag or value + 1\n";
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.path.as_str(),
                candidate.span,
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
                candidate.line,
                candidate.column,
                candidate.symbol.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        candidates,
        vec![
            (
                "pkg/sample.py",
                ByteSpan {
                    start: 11,
                    length: 2,
                },
                "==",
                "!=",
                "compare_eq_ne",
                1,
                11,
                None,
            ),
            (
                "pkg/sample.py",
                ByteSpan {
                    start: 56,
                    length: 8,
                },
                "not flag",
                "flag",
                "remove_not",
                3,
                11,
                Some("decide"),
            ),
            (
                "pkg/sample.py",
                ByteSpan {
                    start: 65,
                    length: 2,
                },
                "or",
                "and",
                "boolean_and_or",
                3,
                20,
                Some("decide"),
            ),
            (
                "pkg/sample.py",
                ByteSpan {
                    start: 74,
                    length: 1,
                },
                "+",
                "-",
                "binary_add_sub",
                3,
                29,
                Some("decide"),
            ),
        ]
    );
}
#[test]
fn removes_not_across_its_complete_ast_operand_range() {
    let output = analyze("result = not (left == right and ready)\n");
    let candidate = output
        .candidates
        .iter()
        .find(|candidate| candidate.operator == "remove_not")
        .expect("not candidate");
    assert_eq!(candidate.original, "not (left == right and ready)");
    assert_eq!(candidate.replacement, "(left == right and ready)");
    assert_eq!(candidate.span.length, candidate.original.len() as u64);
}

#[test]
fn classifies_unary_signs_from_the_ast_not_preceding_tokens() {
    let output = analyze("result = left * -right + +other\n");
    let signs: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.original.as_str(), "+" | "-"))
        .map(|candidate| (candidate.original.as_str(), candidate.operator.as_str()))
        .collect();
    assert_eq!(
        signs,
        vec![
            ("-", "unary_sign"),
            ("+", "binary_add_sub"),
            ("+", "unary_sign"),
        ]
    );
}

#[test]
fn assigns_ast_scopes_to_decorators_async_definitions_and_not_following_code() {
    let source = "class Outer:\n    @decorator(left == right)\n    async def method(self):\n        return not (ready and enabled)\nafter = left == right\n";
    let output = analyze(source);
    let observed: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.symbol.as_deref(),
                candidate.line,
            )
        })
        .collect();
    assert_eq!(
        observed,
        vec![
            ("==", Some("Outer.method"), 2),
            ("not (ready and enabled)", Some("Outer.method"), 4),
            ("and", Some("Outer.method"), 4),
            ("==", None, 5),
        ]
    );
}

#[test]
fn emits_each_remaining_mvp_operator() {
    let source = "def f(a, b, xs):\n    a != b\n    a < b\n    a <= b\n    a > b\n    a >= b\n    a in xs\n    a is b\n    a or b\n    a + b\n    a - b\n    a / b\n    while a:\n        break\n        continue\n";
    let output = analyze(source);
    let pairs: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("!=", "=="),
            ("<", "<="),
            ("<=", "<"),
            (">", ">="),
            (">=", ">"),
            ("in", "not in"),
            ("is", "is not"),
            ("or", "and"),
            ("+", "-"),
            ("-", "+"),
            ("/", "*"),
            ("break", "continue"),
            ("continue", "break")
        ]
    );
}

#[test]
fn reports_unicode_code_point_columns_without_changing_byte_spans() {
    let source = "# 日本語\n値 = left == right  # adjacent comment\n";
    let candidate = analyze(source).candidates.remove(0);
    assert_eq!(
        candidate.span,
        ByteSpan {
            start: 23,
            length: 2
        }
    );
    assert_eq!((candidate.line, candidate.column), (2, 9));
    let mut changed = source.as_bytes().to_vec();
    let start = usize::try_from(candidate.span.start).unwrap();
    changed.splice(start..start + 2, candidate.replacement.bytes());
    assert_eq!(
        String::from_utf8(changed).unwrap(),
        "# 日本語\n値 = left != right  # adjacent comment\n"
    );
}

#[test]
fn assigns_nested_definition_symbols_and_init_module_selectors() {
    let source = "class Outer:\n    def method(self, left, right):\n        return left == right\n";
    let output = analyze_with(
        Utf8Path::new("pkg/sub/__init__.py"),
        &[],
        &["sub:Outer.method".to_owned()],
        10_000,
        source,
    );
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("Outer.method"));
}

#[test]
fn filters_candidates_by_line_and_symbol() {
    let source = "def selected(a, b, c, d):\n    first = a == b\n    return c == d\n\ndef other(a, b):\n    return a == b\n";
    let output = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[
            LineRange { start: 2, end: 2 },
            LineRange { start: 6, end: 6 },
        ],
        &["pkg.sample:selected".to_owned()],
        10_000,
        source,
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|candidate| (candidate.line, candidate.symbol.as_deref()))
            .collect::<Vec<_>>(),
        vec![(2, Some("selected"))]
    );
}

#[test]
fn reports_invalid_syntax_without_candidates() {
    let output = analyze("def broken(:\n");
    assert!(output.candidates.is_empty());
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::InvalidSyntax
    );
    assert!(!output.truncated);
}

#[test]
fn truncates_after_selected_candidates_and_emits_limit_diagnostic() {
    let output = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[],
        &[],
        2,
        "a == b and c == d\n",
    );
    assert_eq!(output.candidates.len(), 2);
    assert!(output.truncated);
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::CandidateLimitExceeded
    );
}
