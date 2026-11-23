//! Sparse admission checks for source edits that introduce Python delimiters.

use super::AnalysisCancelled;
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange, TextSize};

// CPython caps delimiters at 200, including redundant parentheses absent
// from the AST. Record only the intervals that exclude up to eight additional
// delimiters, so shallow files need no per-token storage.
#[derive(Default)]
pub(super) struct DelimiterDepth {
    forbidden: [Vec<TextRange>; 8],
}

impl DelimiterDepth {
    pub(super) fn build(
        tokens: &Tokens,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        const LIMIT: usize = 200;
        let near_limit = |depth| LIMIT.checked_sub(depth).filter(|index| *index < 8);
        let mut index = Self::default();
        let mut depth = 0usize;
        let mut openings = [None; 8];
        for token in tokens {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            match token.kind() {
                TokenKind::Lpar | TokenKind::Lbrace | TokenKind::Lsqb => {
                    depth += 1;
                    if let Some(bucket) = near_limit(depth) {
                        openings[bucket] = Some(token.start());
                    }
                }
                TokenKind::Rpar | TokenKind::Rbrace | TokenKind::Rsqb => {
                    if let Some(bucket) = near_limit(depth)
                        && let Some(start) = openings[bucket].take()
                    {
                        index.forbidden[bucket].push(TextRange::new(start, token.end()));
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        Ok(index)
    }

    pub(super) fn can_enclose(&self, range: TextRange, added: usize) -> bool {
        self.first_interval_after(range.start(), added)
            .is_none_or(|interval| interval.start() >= range.end())
    }

    pub(super) fn room_before(&self, offset: TextSize, added: usize) -> bool {
        self.first_interval_after(offset, added)
            .is_none_or(|interval| interval.start() >= offset)
    }

    fn first_interval_after(&self, offset: TextSize, added: usize) -> Option<TextRange> {
        let intervals = &self.forbidden[added - 1];
        let next = intervals.partition_point(|interval| interval.end() <= offset);
        intervals.get(next).copied()
    }
}
