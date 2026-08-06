use super::{AnalyzeRequest, LineIndex, analyze_source, analyze_source_cancellable};
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

#[test]
fn clean_collection_builtin_calls_remain_eligible_for_ast_collection() {
    let source = "result = any(items)\n";
    let parsed = parse_module(source).expect("clean builtin call parses");
    let facts = super::AstFacts::from_module(parsed.syntax(), parsed.tokens());

    assert!(!facts.is_builtin_bound("any"));
}

#[test]
fn comprehension_targets_bind_collection_builtins() {
    let source = "result = [list(item) for list in factories]\n";
    let parsed = parse_module(source).expect("comprehension fixture parses");
    let facts = super::AstFacts::from_module(parsed.syntax(), parsed.tokens());

    assert!(facts.is_builtin_bound("list"));
}

#[test]
fn annotated_augmented_and_lambda_bindings_are_conservative() {
    let source = "tuple: object = custom_tuple\nmax += value\nmapper = lambda *, sorted, **reversed: (sorted, reversed)\n";
    let parsed = parse_module(source).expect("binding fixture parses");
    let facts = super::AstFacts::from_module(parsed.syntax(), parsed.tokens());

    for name in ["tuple", "max", "sorted", "reversed"] {
        assert!(facts.is_builtin_bound(name), "expected {name} to be bound");
    }
}

#[test]
fn exception_bindings_are_conservative() {
    let clean = parse_module("try:\n    work()\nexcept ValueError:\n    pass\n")
        .expect("clean exception fixture parses");
    let clean_facts = super::AstFacts::from_module(clean.syntax(), clean.tokens());
    assert!(!clean_facts.is_exception_bound("ValueError"));

    for (source, name) in [
        ("ValueError = Custom\n", "ValueError"),
        ("def handle(TypeError):\n    pass\n", "TypeError"),
        ("from helpers import KeyError\n", "KeyError"),
        ("from helpers import *\n", "IndexError"),
        (
            "match value:\n    case PermissionError:\n        pass\n",
            "PermissionError",
        ),
        (
            "try:\n    work()\nexcept OSError as FileNotFoundError:\n    pass\n",
            "FileNotFoundError",
        ),
    ] {
        let parsed = parse_module(source).expect("exception binding fixture parses");
        let facts = super::AstFacts::from_module(parsed.syntax(), parsed.tokens());
        assert!(
            facts.is_exception_bound(name),
            "expected {name} bound in {source:?}"
        );
    }
}

#[test]
fn exception_type_pair_candidates_are_curated_and_syntax_directed() {
    let source = concat!(
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
    );
    let output = analyze(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "exception_type_pair")
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();
    assert_eq!(
        actual,
        vec![
            ("ValueError", "TypeError"),
            ("TypeError", "ValueError"),
            ("KeyError", "IndexError"),
            ("KeyError", "AttributeError"),
            ("IndexError", "KeyError"),
            ("AttributeError", "KeyError"),
            ("FileNotFoundError", "PermissionError"),
            ("PermissionError", "FileNotFoundError"),
            ("ConnectionError", "TimeoutError"),
            ("TimeoutError", "ConnectionError"),
            ("ImportError", "ModuleNotFoundError"),
            ("ModuleNotFoundError", "ImportError"),
            ("ZeroDivisionError", "OverflowError"),
            ("OverflowError", "ZeroDivisionError"),
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
fn exception_risky_candidates_require_explicit_selection_and_reparse() {
    let source = concat!(
        "try:\n    work()\n",
        "except:\n    pass\n",
        "except Exception:\n    pass\n",
        "except BaseException:\n    pass\n",
        "except (ValueError,):\n    pass\n",
        "except (ValueError, TypeError,):\n    pass\n",
        "except (ValueError, # keep this comment\n",
        "        TypeError,):\n    pass\n",
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
    let parsed = parse_module(source).expect("shadowing fixture parses");
    let facts = super::AstFacts::from_module(parsed.syntax(), parsed.tokens());
    for name in [
        "any",
        "all",
        "list",
        "tuple",
        "set",
        "frozenset",
        "min",
        "max",
        "sorted",
        "reversed",
    ] {
        assert!(facts.is_builtin_bound(name), "expected {name} to be bound");
    }

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
        vec![
            (5, "binary_add_sub"),
            (7, "binary_mul_div"),
            (8, "binary_add_sub"),
        ]
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
