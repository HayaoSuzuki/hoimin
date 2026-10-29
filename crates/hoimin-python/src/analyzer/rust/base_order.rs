//! Reorder simple direct bases while retaining trivia in each source slot.
use std::collections::HashSet;

use ruff_python_ast::{Expr, StmtClassDef};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn span(class: &StmtClassDef) -> Option<TextRange> {
    let arguments = class.arguments.as_ref()?;
    if arguments.args.len() < 2
        || !arguments.keywords.is_empty()
        || !class.decorator_list.is_empty()
        || class.type_params.is_some()
    {
        return None;
    }
    Some(TextRange::new(
        arguments.args.first()?.start(),
        arguments.args.last()?.end(),
    ))
}

pub(super) fn replacement(
    class: &StmtClassDef,
    source: &str,
    cancelled: &impl Fn() -> bool,
) -> Option<String> {
    let range = span(class)?;
    let bases = &class.arguments.as_ref()?.args;
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
        hasher.update(super::source_text(source, range)?.as_bytes());
        let mut random = hasher.finalize_xof();
        for index in (1..order.len()).rev() {
            if cancelled() {
                return None;
            }
            let mut bytes = [0; 8];
            random.fill(&mut bytes);
            let modulus = u64::try_from(index + 1).ok()?;
            let position = usize::try_from(u64::from_le_bytes(bytes) % modulus).ok()?;
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
        result.push_str(source.get(cursor..usize::from(base.start()))?);
        result.push_str(fragments[order[index]]);
        cursor = usize::from(base.end());
    }
    result.push_str(source.get(cursor..end)?);
    #[cfg(test)]
    super::whole_expression_selection_tests::record_replacement(
        super::MutationOperator::BaseOrderShuffle,
        &result,
    );
    Some(result)
}
