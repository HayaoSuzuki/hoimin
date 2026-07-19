use camino::Utf8Path;
use hoimin_core::{ByteSpan, LineRange};
use ruff_python_ast::token::TokenKind;
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
        } else if text == "not" {
            let expression = next.unwrap();
            let expression_start = usize::from(expression.range().start());
            let expression_end = usize::from(expression.range().end());
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
        } else if let Some((replacement, operator)) =
            replacement(text, unary_sign(tokens.as_slice(), index))
        {
            (end, replacement.to_owned(), operator)
        } else {
            continue;
        };
        let original = source[start..span_end].to_owned();
        let (line, column) = line_and_column(source, start);
        let symbol = scope_at(source, start);
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

fn unary_sign(tokens: &[&ruff_python_ast::token::Token], index: usize) -> bool {
    let Some(previous) = index.checked_sub(1).and_then(|i| tokens.get(i)) else {
        return true;
    };
    matches!(
        previous.kind(),
        TokenKind::Lpar
            | TokenKind::Comma
            | TokenKind::Equal
            | TokenKind::Return
            | TokenKind::Colon
    )
}

fn line_and_column(source: &str, offset: usize) -> (u32, u32) {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len()) as u32;
    (line, column)
}

fn scope_at(source: &str, offset: usize) -> Option<String> {
    let mut scopes: Vec<(usize, String)> = Vec::new();
    let mut consumed = 0;
    for line in source.split_inclusive('\n') {
        if consumed > offset {
            break;
        }
        let indent = line.len() - line.trim_start().len();
        let words: Vec<_> = line.split_whitespace().collect();
        if let Some(name) = words
            .get(1)
            .filter(|_| matches!(words.first(), Some(&"def") | Some(&"class")))
        {
            while scopes.last().is_some_and(|(depth, _)| *depth >= indent) {
                scopes.pop();
            }
            scopes.push((
                indent,
                name.trim_end_matches('(')
                    .trim_end_matches(':')
                    .split('(')
                    .next()
                    .unwrap()
                    .to_owned(),
            ));
        } else {
            while scopes
                .last()
                .is_some_and(|(depth, _)| *depth > indent && !line.trim().is_empty())
            {
                scopes.pop();
            }
        }
        consumed += line.len();
    }
    (!scopes.is_empty()).then(|| {
        scopes
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
            .join(".")
    })
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
