//! Reference identities from the existing regex scanner.
use super::pattern::Edit;
use std::ops::Range;

#[derive(Default)]
pub(super) struct ReferenceIndex {
    captures: Vec<usize>,
    references: Vec<(usize, usize, bool)>,
}
impl ReferenceIndex {
    pub(super) fn capture(&mut self, position: usize) {
        self.captures.push(position);
    }

    pub(super) fn record(&mut self, position: usize, capture: usize, numeric: bool) {
        self.references.push((position, capture, numeric));
    }

    /// Removing captures can erase a named reference's target or rebind a
    /// numeric reference. References removed with their targets are harmless.
    pub(super) fn retain_deletions(
        self,
        edits: &mut Vec<Edit>,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<()> {
        if edits.is_empty() || self.captures.is_empty() || self.references.is_empty() {
            return Some(());
        }
        let mut named_uses: Vec<Option<Range<usize>>> = vec![None; self.captures.len()];
        let mut numeric_uses = Vec::new();
        for (position, capture, numeric) in self.references {
            if cancelled() {
                return None;
            }
            if numeric {
                numeric_uses.push((position, capture));
            } else {
                let uses = named_uses[capture - 1].get_or_insert(position..position + 1);
                uses.end = position + 1;
            }
        }
        let prefix_max: Vec<_> = numeric_uses
            .iter()
            .scan(0, |max, &(_, capture)| {
                *max = (*max).max(capture);
                Some(*max)
            })
            .collect();
        let suffix_max: Vec<_> = numeric_uses
            .iter()
            .rev()
            .scan(0, |max, &(_, capture)| {
                *max = (*max).max(capture);
                Some(*max)
            })
            .collect();
        edits.retain(|edit| {
            if cancelled() {
                return false;
            }
            let start = self.captures.partition_point(|&at| at < edit.span.start);
            let end = self.captures.partition_point(|&at| at < edit.span.end);
            if start == end {
                return true;
            }
            let prior = numeric_uses.partition_point(|&(at, _)| at < edit.span.start);
            let later = numeric_uses.partition_point(|&(at, _)| at < edit.span.end);
            let prior_max = prior.checked_sub(1).map_or(0, |i| prefix_max[i]);
            let later_max = (numeric_uses.len() - later)
                .checked_sub(1)
                .map_or(0, |i| suffix_max[i]);
            prior_max.max(later_max) <= start
                && named_uses[start..end]
                    .iter()
                    .flatten()
                    .all(|uses| uses.start >= edit.span.start && uses.end <= edit.span.end)
        });
        (!cancelled()).then_some(())
    }
}
