//! Eligibility and source boundaries for one adjacent exchange in a single suite.
use super::{
    AstCandidateCollector, Expr, ExprContext, HashSet, MutationOperator, Ranged, Stmt, TextRange,
    TokenKind, Visitor, byte_range, selected_source_start, visitor,
};

#[derive(Default)]
struct Footprint<'ast> {
    reads: HashSet<&'ast str>,
    write: Option<&'ast str>,
    effect: bool,
    blocked: bool,
}

impl<'ast> Footprint<'ast> {
    fn for_statement(statement: &'ast Stmt, cancelled: &impl Fn() -> bool) -> Option<Self> {
        let mut info = Self::default();
        match statement {
            Stmt::Expr(s) => {
                let call = match s.value.as_ref() {
                    Expr::Call(call) => call,
                    Expr::Await(a) => a.value.as_call_expr()?,
                    _ => return None,
                };
                if let Expr::Attribute(attr) = call.func.as_ref()
                    && let Expr::Call(base) = attr.value.as_ref()
                    && base
                        .func
                        .as_name_expr()
                        .is_some_and(|name| name.id == "super")
                {
                    return None;
                }
            }
            Stmt::Assign(s) if s.targets.len() == 1 => info.target(&s.targets[0], false)?,
            Stmt::AugAssign(s) => info.target(&s.target, true)?,
            Stmt::Assert(_) => info.effect = true,
            _ => return None,
        }
        let mut visitor = ReadNames {
            info: &mut info,
            cancelled,
        };
        visitor::walk_stmt(&mut visitor, statement);
        (!info.blocked).then_some(info)
    }

    fn target(&mut self, expr: &'ast Expr, augmented: bool) -> Option<()> {
        match expr {
            Expr::Name(name) => {
                self.write = Some(name.id.as_str());
                if augmented {
                    self.reads.insert(name.id.as_str());
                    self.effect = true;
                }
            }
            Expr::Attribute(_) | Expr::Subscript(_) => self.effect = true,
            _ => return None,
        }
        Some(())
    }
}

struct ReadNames<'a, 'ast, F> {
    info: &'a mut Footprint<'ast>,
    cancelled: &'a F,
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for ReadNames<'_, 'ast, F> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if self.info.blocked || (self.cancelled)() {
            self.info.blocked = true;
            return;
        }
        match expr {
            Expr::Name(name) if name.ctx == ExprContext::Load => {
                self.info.reads.insert(name.id.as_str());
            }
            Expr::Call(_) | Expr::Await(_) => self.info.effect = true,
            Expr::Yield(_)
            | Expr::YieldFrom(_)
            | Expr::Named(_)
            | Expr::Lambda(_)
            | Expr::Generator(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_) => {
                self.info.blocked = true;
                return;
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

impl<F: Fn() -> bool> AstCandidateCollector<'_, F> {
    pub(super) fn collect_swap(
        &mut self,
        first: &Stmt,
        second: &Stmt,
        trailing_directive: &mut Option<(u32, bool)>,
    ) {
        if self.check_cancelled() {
            return;
        }
        let range = TextRange::new(first.start(), second.end());
        if !selected_source_start(range, self.line_index, self.facts, self.selection) {
            return;
        }
        let Some(a) = Footprint::for_statement(first, self.cancelled) else {
            return;
        };
        let Some(b) = Footprint::for_statement(second, self.cancelled) else {
            return;
        };
        if !(a.effect || b.effect)
            || a.write.is_some_and(|name| b.reads.contains(name))
            || b.write.is_some_and(|name| a.reads.contains(name))
        {
            return;
        }
        let left = &self.source[byte_range(first.range())];
        let right = &self.source[byte_range(second.range())];
        if left == right {
            return;
        }
        let tokens = self.facts.tokens.expect("checked parse");
        for token in tokens.in_range(range) {
            if self.check_cancelled() {
                return;
            }
            if token.kind() == TokenKind::Comment
                && directive(&self.source[byte_range(token.range())])
            {
                return;
            }
        }
        // An end-of-line directive applies to its physical line. Sibling pairs
        // advance in source order, so rescan a shared semicolon line only once.
        let (line, _) = self.line_index.line_and_column(usize::from(second.end()));
        if trailing_directive.is_none_or(|(seen, _)| seen != line) {
            let mut attached = false;
            for token in tokens.after(second.end()) {
                if self.check_cancelled() {
                    return;
                }
                if matches!(
                    token.kind(),
                    TokenKind::Newline | TokenKind::NonLogicalNewline
                ) || self
                    .line_index
                    .line_and_column(usize::from(token.start()))
                    .0
                    != line
                {
                    break;
                }
                if token.kind() == TokenKind::Comment {
                    attached = directive(&self.source[byte_range(token.range())]);
                    break;
                }
            }
            *trailing_directive = Some((line, attached));
        }
        if trailing_directive.is_some_and(|(_, attached)| attached) {
            return;
        }
        let between = &self.source[byte_range(TextRange::new(first.end(), second.start()))];
        self.add_candidate(
            range,
            format!("{right}{between}{left}"),
            MutationOperator::StatementOrderSwap,
        );
    }
}

fn directive(comment: &str) -> bool {
    let text = comment.trim_start_matches('#').trim_start();
    [
        "type:",
        "noqa",
        "ruff:",
        "fmt:",
        "pragma:",
        "pylint:",
        "pyright:",
        "mypy:",
        "isort:",
        "yapf:",
        "nosec",
        "coverage:",
    ]
    .iter()
    .any(|prefix| {
        text.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    })
}
