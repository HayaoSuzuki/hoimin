use std::collections::HashSet;

use camino::Utf8Path;
use hoimin_core::{ByteSpan, LineRange};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{Expr, ModModule, Stmt, UnaryOp, visitor};
use ruff_python_parser::parse_module;
use ruff_text_size::Ranged;

use super::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};

pub struct AnalyzeRequest<'a> {
    pub path: &'a Utf8Path,
    pub lines: &'a [LineRange],
    pub symbols: &'a [String],
    pub max_candidates: usize,
}

pub struct AnalyzerOutput {
    pub candidates: Vec<AnalyzerCandidate>,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub truncated: bool,
}

pub fn analyze_source(request: &AnalyzeRequest<'_>, source: &str) -> AnalyzerOutput {
    let parsed = match parse_module(source) {
        Ok(parsed) => parsed,
        Err(_) => return invalid_syntax(request.path),
    };
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens());
    let mut candidates = Vec::new();
    let tokens: Vec<_> = parsed.tokens().iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        let range = token.range();
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let text = &source[start..end];
        let previous = index.checked_sub(1).and_then(|i| tokens.get(i));
        let next = tokens.get(index + 1);
        let (span_end, replacement, operator) = if text == "not"
            && next.is_some_and(|next| {
                &source[usize::from(next.range().start())..usize::from(next.range().end())] == "in"
            }) {
            (
                usize::from(next.unwrap().range().end()),
                "in".to_owned(),
                "membership",
            )
        } else if text == "is"
            && next.is_some_and(|next| {
                &source[usize::from(next.range().start())..usize::from(next.range().end())] == "not"
            })
        {
            (
                usize::from(next.unwrap().range().end()),
                "is".to_owned(),
                "identity",
            )
        } else if text == "not"
            && previous.is_some_and(|previous| {
                &source[usize::from(previous.range().start())..usize::from(previous.range().end())]
                    == "is"
            })
        {
            continue;
        } else if let Some((expression_start, expression_end)) = facts.not_operand_range(start) {
            (
                expression_end,
                source[expression_start..expression_end].to_owned(),
                "remove_not",
            )
        } else if previous.is_some_and(|previous| {
            &source[usize::from(previous.range().start())..usize::from(previous.range().end())]
                == "not"
                && matches!(text, "in" | "is")
        }) {
            continue;
        } else if let Some((replacement, operator)) = replacement(text, facts.is_unary_sign(start))
        {
            (end, replacement.to_owned(), operator)
        } else {
            continue;
        };
        let original = source[start..span_end].to_owned();
        let (line, column) = line_and_column(source, start);
        let symbol = facts.scope_at(start);
        if selected(request, line, symbol.as_deref()) {
            candidates.push(AnalyzerCandidate {
                path: request.path.to_owned(),
                span: ByteSpan {
                    start: start as u64,
                    length: (span_end - start) as u64,
                },
                original,
                replacement,
                operator: operator.to_owned(),
                line,
                column,
                symbol,
            });
        }
    }
    candidates.sort_by_key(|candidate| candidate.span.start);
    let truncated = candidates.len() > request.max_candidates;
    if truncated {
        candidates.truncate(request.max_candidates);
    }
    let diagnostics = truncated
        .then(|| AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::CandidateLimitExceeded,
            path: Some(request.path.to_owned()),
            line: None,
            column: None,
            message: None,
        })
        .into_iter()
        .collect();
    AnalyzerOutput {
        candidates,
        diagnostics,
        truncated,
    }
}

fn invalid_syntax(path: &Utf8Path) -> AnalyzerOutput {
    AnalyzerOutput {
        candidates: Vec::new(),
        diagnostics: vec![AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::InvalidSyntax,
            path: Some(path.to_owned()),
            line: None,
            column: None,
            message: None,
        }],
        truncated: false,
    }
}

fn replacement(text: &str, unary: bool) -> Option<(&'static str, &'static str)> {
    let result = match text {
        "==" => ("!=", "compare_eq_ne"),
        "!=" => ("==", "compare_eq_ne"),
        "<" => ("<=", "compare_order"),
        "<=" => ("<", "compare_order"),
        ">" => (">=", "compare_order"),
        ">=" => (">", "compare_order"),
        "in" => ("not in", "membership"),
        "is" => ("is not", "identity"),
        "and" => ("or", "boolean_and_or"),
        "or" => ("and", "boolean_and_or"),
        "+=" => ("-=", "augmented_add_sub"),
        "-=" => ("+=", "augmented_add_sub"),
        "*" => ("/", "binary_mul_div"),
        "/" => ("*", "binary_mul_div"),
        "//" => ("%", "binary_floor_mod"),
        "%" => ("//", "binary_floor_mod"),
        "break" => ("continue", "break_continue"),
        "continue" => ("break", "break_continue"),
        "True" => ("False", "boolean_literal"),
        "False" => ("True", "boolean_literal"),
        "+" if unary => ("-", "unary_sign"),
        "-" if unary => ("+", "unary_sign"),
        "+" => ("-", "binary_add_sub"),
        "-" => ("+", "binary_add_sub"),
        _ => return None,
    };
    Some(result)
}

fn line_and_column(source: &str, offset: usize) -> (u32, u32) {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len()) as u32;
    (line, column)
}

fn selected(request: &AnalyzeRequest<'_>, line: u32, symbol: Option<&str>) -> bool {
    if request.lines.is_empty() && request.symbols.is_empty() {
        return true;
    }
    request
        .lines
        .iter()
        .any(|range| range.start <= line && line <= range.end)
        || request.symbols.iter().any(|selector| {
            let Some((module, qualname)) = selector.rsplit_once(':') else {
                return false;
            };
            let current_module = module_name(request.path);
            (current_module == module || current_module.ends_with(&format!(".{module}")))
                && symbol.is_some_and(|symbol| {
                    symbol == qualname || symbol.starts_with(&format!("{qualname}."))
                })
        })
}

fn module_name(path: &Utf8Path) -> String {
    let mut parts: Vec<_> = path.with_extension("").iter().map(str::to_owned).collect();
    if parts.last().is_some_and(|part| part == "__init__") {
        parts.pop();
    }
    parts.join(".")
}

#[derive(Default)]
struct AstFacts<'tokens> {
    unary_sign_starts: HashSet<usize>,
    not_operands: Vec<(usize, usize, usize)>,
    scopes: Vec<ScopeRange>,
    qualname: Vec<String>,
    tokens: Option<&'tokens ruff_python_ast::token::Tokens>,
}
struct ScopeRange {
    start: usize,
    end: usize,
    symbol: String,
}

impl<'tokens> AstFacts<'tokens> {
    fn from_module(module: &ModModule, tokens: &'tokens ruff_python_ast::token::Tokens) -> Self {
        let mut facts = Self {
            tokens: Some(tokens),
            ..Self::default()
        };
        for statement in &module.body {
            facts.visit_stmt(statement);
        }
        facts
    }

    fn not_operand_range(&self, start: usize) -> Option<(usize, usize)> {
        self.not_operands
            .iter()
            .find_map(|(not_start, operand_start, operand_end)| {
                (*not_start == start).then_some((*operand_start, *operand_end))
            })
    }

    fn is_unary_sign(&self, start: usize) -> bool {
        self.unary_sign_starts.contains(&start)
    }

    fn scope_at(&self, offset: usize) -> Option<String> {
        self.scopes
            .iter()
            .filter(|scope| scope.start <= offset && offset < scope.end)
            .max_by_key(|scope| scope.start)
            .map(|scope| scope.symbol.clone())
    }

    fn visit_definition(
        &mut self,
        name: &str,
        range: ruff_text_size::TextRange,
        decorators: &[ruff_python_ast::Decorator],
        statement: &Stmt,
    ) {
        let start = decorators
            .iter()
            .map(|decorator| usize::from(decorator.range().start()))
            .min()
            .unwrap_or_else(|| usize::from(range.start()));
        self.qualname.push(name.to_owned());
        self.scopes.push(ScopeRange {
            start,
            end: usize::from(range.end()),
            symbol: self.qualname.join("."),
        });
        visitor::walk_stmt(self, statement);
        self.qualname.pop();
    }
}

impl<'ast, 'tokens> Visitor<'ast> for AstFacts<'tokens> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::FunctionDef(definition) => self.visit_definition(
                definition.name.as_str(),
                definition.range(),
                &definition.decorator_list,
                statement,
            ),
            Stmt::ClassDef(definition) => self.visit_definition(
                definition.name.as_str(),
                definition.range(),
                &definition.decorator_list,
                statement,
            ),
            _ => visitor::walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if let Expr::UnaryOp(unary) = expression {
            let start = usize::from(unary.range().start());
            match unary.op {
                UnaryOp::Not => {
                    let operand_range = ruff_python_ast::token::parenthesized_range(
                        unary.operand.as_ref().into(),
                        unary.into(),
                        self.tokens.expect("parser tokens are set"),
                    )
                    .unwrap_or_else(|| unary.operand.range());
                    self.not_operands.push((
                        start,
                        usize::from(operand_range.start()),
                        usize::from(operand_range.end()),
                    ));
                }
                UnaryOp::UAdd | UnaryOp::USub => {
                    self.unary_sign_starts.insert(start);
                }
                UnaryOp::Invert => {}
            }
        }
        visitor::walk_expr(self, expression);
    }
}
