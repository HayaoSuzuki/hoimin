use std::collections::BTreeSet;
use std::process::Command;

use camino::Utf8Path;
use hoimin_core::{ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateDescriptor};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Operator, Stmt};
use ruff_text_size::{Ranged, TextRange};

pub const SAMPLE_SHA: &str = "409ed955c0695ec04976842ba9fd22c64c03c33a";
pub const TARGET: &str = "src/proofreadtools/checks/_variant_candidates.py";
pub const SYMBOL: &str = "_length_ratio_upper_bound";

pub fn validate_sample(root: &Utf8Path) -> Result<(), String> {
    for (args, expected) in [
        (vec!["rev-parse", "HEAD"], SAMPLE_SHA),
        (vec!["status", "--porcelain", "--untracked-files=no"], ""),
    ] {
        let result = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() || String::from_utf8_lossy(&result.stdout).trim() != expected {
            return Err("sample must be the clean pinned checkout".into());
        }
    }
    Ok(())
}

struct Edits<'source> {
    source: &'source str,
    entries: BTreeSet<(usize, usize, String, &'static str)>,
}

impl Edits<'_> {
    fn text(&self, range: TextRange) -> &str {
        &self.source[usize::from(range.start())..usize::from(range.end())]
    }

    fn add(&mut self, range: TextRange, replacement: String, kind: &'static str) {
        if self.text(range) != replacement {
            self.entries.insert((
                usize::from(range.start()),
                usize::from(range.end()),
                replacement,
                kind,
            ));
        }
    }
}

impl<'ast> Visitor<'ast> for Edits<'_> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if let Expr::BinOp(binary) = expr {
            let left = self.text(binary.left.range()).to_owned();
            let right = self.text(binary.right.range()).to_owned();
            // One AST node is replaced; no independent edits are composed.
            for replacement in [left.clone(), right.clone()] {
                self.add(expr.range(), format!("({replacement})"), "research.operand");
            }
            for (operator, op) in [
                ("+", Operator::Add),
                ("-", Operator::Sub),
                ("*", Operator::Mult),
                ("/", Operator::Div),
            ] {
                if binary.op == op {
                    continue;
                }
                self.add(
                    expr.range(),
                    format!("({left}) {operator} ({right})"),
                    "research.binary",
                );
            }
        }
        if let Expr::Call(call) = expr
            && matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "min")
        {
            for arg in &call.arguments.args {
                self.add(
                    expr.range(),
                    format!("({})", self.text(arg.range())),
                    "research.call_argument",
                );
            }
            self.add(call.func.range(), "max".into(), "research.call_function");
        }
        visitor::walk_expr(self, expr);
    }
}

pub fn candidate_space(source: &str) -> Result<Vec<CandidateDescriptor>, String> {
    candidate_space_for(source, TARGET, SYMBOL)
}

pub fn candidate_space_for(
    source: &str,
    target: &str,
    symbol: &str,
) -> Result<Vec<CandidateDescriptor>, String> {
    let parsed = ruff_python_parser::parse_module(source).map_err(|e| e.to_string())?;
    let function = parsed
        .syntax()
        .body
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::FunctionDef(def) if def.name.as_str() == symbol => Some(def),
            _ => None,
        })
        .ok_or_else(|| "fixed target function is absent".to_owned())?;
    let mut edits = Edits {
        source,
        entries: BTreeSet::new(),
    };
    for stmt in &function.body {
        edits.visit_stmt(stmt);
    }
    let mut result = Vec::new();
    for (start, end, replacement, operator) in edits.entries {
        let mut changed = source.to_owned();
        changed.replace_range(start..end, &replacement);
        if ruff_python_parser::parse_module(&changed).is_err() {
            continue;
        }
        let prefix = &source[..start];
        result.push(CandidateDescriptor {
            schema_version: CANDIDATE_SCHEMA_VERSION,
            path: target.into(),
            span: ByteSpan {
                start: u64::try_from(start).map_err(|e| e.to_string())?,
                length: u64::try_from(end - start).map_err(|e| e.to_string())?,
            },
            original: source[start..end].into(),
            replacement,
            operator: operator.into(),
            line: u32::try_from(prefix.bytes().filter(|b| *b == b'\n').count() + 1)
                .map_err(|e| e.to_string())?,
            column: u32::try_from(prefix.rsplit('\n').next().unwrap_or("").chars().count())
                .map_err(|e| e.to_string())?,
            symbol: Some(symbol.into()),
            file_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        });
    }
    Ok(result)
}

pub fn seeded_runner(seed: u64) -> TestRunner {
    let mut bytes = [0; 32];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    TestRunner::new_with_rng(
        Config::default(),
        TestRng::from_seed(RngAlgorithm::ChaCha, &bytes),
    )
}

pub fn shrink_tree(
    source: &str,
    candidate: &CandidateDescriptor,
) -> Result<(Vec<CandidateDescriptor>, proptest::num::usize::BinarySearch), String> {
    struct Nodes(usize);
    impl<'ast> Visitor<'ast> for Nodes {
        fn visit_expr(&mut self, expr: &'ast Expr) {
            self.0 += 1;
            visitor::walk_expr(self, expr);
        }
    }
    let mut space = candidate_space(source)?;
    let mut ranked = Vec::new();
    for entry in space.drain(..) {
        let expression =
            ruff_python_parser::parse_module(&entry.replacement).map_err(|e| e.to_string())?;
        let mut nodes = Nodes(0);
        for stmt in &expression.syntax().body {
            nodes.visit_stmt(stmt);
        }
        ranked.push((nodes.0, entry.replacement.len(), entry));
    }
    ranked.sort_by(|a, b| (a.0, a.1, &a.2.replacement).cmp(&(b.0, b.1, &b.2.replacement)));
    let space = ranked
        .into_iter()
        .map(|(_, _, entry)| entry)
        .collect::<Vec<_>>();
    let index = space
        .iter()
        .position(|entry| entry == candidate)
        .ok_or("candidate outside generated space")?;
    Ok((space, proptest::num::usize::BinarySearch::new(index)))
}

pub fn generate_candidates(
    source: &str,
    seed: u64,
    limit: usize,
) -> Result<Vec<CandidateDescriptor>, String> {
    let mut space = candidate_space(source)?;
    let mut runner = seeded_runner(seed);
    let mut chosen = Vec::new();
    while !space.is_empty() && chosen.len() < limit {
        let index = (0..space.len())
            .new_tree(&mut runner)
            .map_err(|e| e.to_string())?
            .current();
        chosen.push(space.remove(index));
    }
    Ok(chosen)
}
