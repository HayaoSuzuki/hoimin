//! Reorder simple direct bases while retaining trivia in each source slot.
use std::collections::HashSet;

use ruff_python_ast::{Expr, StmtClassDef};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn span(class: &StmtClassDef) -> Option<TextRange> {
    let arguments = class.arguments.as_ref()?;
    let [first, .., last] = arguments.args.as_ref() else {
        return None;
    };
    if !arguments.keywords.is_empty()
        || !class.decorator_list.is_empty()
        || class.type_params.is_some()
    {
        return None;
    }
    Some(TextRange::new(first.start(), last.end()))
}

pub(super) fn replacement(
    class: &StmtClassDef,
    source: &str,
    cancelled: &impl Fn() -> bool,
) -> Option<String> {
    let range = span(class)?;
    // A successful span requires at least two explicit class arguments.
    let bases = &class
        .arguments
        .as_ref()
        .expect("span requires arguments")
        .args;
    let original = super::source_text(source, range)?;
    let mut names = HashSet::new();
    let mut fragments = Vec::with_capacity(bases.len());
    for base in bases {
        if cancelled() {
            return None;
        }
        let Expr::Name(name) = base else { return None };
        // Ruff normalizes identifiers, so K and Kelvin-sign spellings are duplicates.
        if !names.insert(name.id.as_str()) {
            return None;
        }
        fragments.push(super::source_text(source, base.range())?);
    }
    let mut order: Vec<_> = (0..bases.len()).collect();
    if order.len() > 2 {
        let mut hasher = blake3::Hasher::new_derive_key("hoimin/base_order_shuffle/v1");
        hasher.update(original.as_bytes());
        let mut random = hasher.finalize_xof();
        for index in (1..order.len()).rev() {
            if cancelled() {
                return None;
            }
            let mut bytes = [0; 8];
            random.fill(&mut bytes);
            // Supported pointer widths fit u64; the remainder fits usize because
            // it is smaller than the original usize bound index + 1.
            let modulus = u64::try_from(index + 1).expect("pointer widths fit u64");
            let position = usize::try_from(u64::from_le_bytes(bytes) % modulus)
                .expect("remainder is smaller than the usize bound");
            order.swap(index, position);
        }
    }
    if order
        .iter()
        .enumerate()
        .all(|(index, value)| index == *value)
    {
        order.swap(0, 1);
    }
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    let mut result = String::with_capacity(end - start);
    let mut cursor = start;
    for (index, base) in bases.iter().enumerate() {
        if cancelled() {
            return None;
        }
        // The parsed bases are ordered, and retaining their fragments validated
        // both endpoints of every intervening source gap.
        result.push_str(&source[cursor..usize::from(base.start())]);
        result.push_str(fragments[order[index]]);
        cursor = usize::from(base.end());
    }
    // The span ends at the final base, so no source tail remains after this loop.
    #[cfg(test)]
    super::whole_expression_selection_tests::record_replacement(
        super::MutationOperator::BaseOrderShuffle,
        &result,
    );
    Some(result)
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run_replacement(
        class: &StmtClassDef,
        source: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<String> {
        replacement(class, source, &cancelled)
    }

    #[rstest::rstest]
    #[case(0, false)]
    #[case(1, false)]
    #[case(2, false)]
    #[case(3, false)]
    #[case(4, false)]
    #[case(5, false)]
    #[case(6, false)]
    #[case(7, false)]
    #[case(8, true)]
    fn cancellation_stops_each_base_shuffle_phase(
        #[case] checkpoint: usize,
        #[case] completed: bool,
    ) {
        let source = "class Child(First, Second, Third): pass\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_class_def_stmt().unwrap();
        let polls = Cell::new(0);
        let result = run_replacement(definition, source, &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });

        let actual = result.is_some();

        assert_eq!(actual, completed);
    }
    #[rstest::rstest]
    #[case("class Child: pass\n", false)]
    #[case("class Child(): pass\n", false)]
    #[case("class Child(First): pass\n", false)]
    #[case("class Child(First, Second): pass\n", true)]
    #[case("class Child(A, B, C): pass\n", true)]
    #[case("class Child(A, B, C, D): pass\n", true)]
    #[case("class Child(First, *bases): pass\n", false)]
    #[case("class Child(First, First): pass\n", false)]
    fn base_shuffle_requires_distinct_unadorned_base_names(
        #[case] source: &str,
        #[case] eligible: bool,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_class_def_stmt().unwrap();

        let actual = run_replacement(definition, source, &|| false).is_some();

        assert_eq!(actual, eligible);
    }

    #[rstest::rstest]
    #[case("")]
    #[case("short")]
    #[case("class Child(AAAAé, Second): pass\n")]
    fn replacement_rejects_source_incompatible_with_its_ast_ranges(#[case] source: &str) {
        let parsed =
            ruff_python_parser::parse_module("class Child(First, Second): pass\n").unwrap();
        let definition = parsed.syntax().body[0].as_class_def_stmt().unwrap();

        let actual = run_replacement(definition, source, &|| false).is_none();

        assert!(actual);
    }
}
