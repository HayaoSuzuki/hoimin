//! Whole-definition edits tied to one original Python byte snapshot.
//! Callers select names; this module owns decorators, suites, codecs and overlap.
use crate::visitor::{self, Visitor};
use hoimin_core::decode_python_source;
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;
use std::collections::BTreeSet;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    pub range: Range<usize>,
}
#[derive(Debug)]
pub struct DefinitionSource {
    raw: Vec<u8>,
    definitions: Vec<Definition>,
    suites: Vec<(Range<usize>, Vec<Range<usize>>)>,
}

#[derive(Default)]
struct Collector {
    parent: Option<Range<usize>>,
    scope: Vec<String>,
    definitions: Vec<(String, Range<usize>)>,
    suites: Vec<(Range<usize>, Vec<Range<usize>>)>,
}
impl<'ast> Visitor<'ast> for Collector {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        let parent = self
            .parent
            .replace(usize::from(statement.start())..usize::from(statement.end()));
        let name = match statement {
            Stmt::FunctionDef(d) => {
                let start = d.decorator_list.first().map_or(d.start(), Ranged::start);
                self.scope.push(d.name.to_string());
                self.definitions.push((
                    self.scope.join("."),
                    usize::from(start)..usize::from(d.end()),
                ));
                Some(d.name.as_str())
            }
            Stmt::ClassDef(d) => {
                self.scope.push(d.name.to_string());
                Some(d.name.as_str())
            }
            _ => None,
        };
        visitor::walk_stmt(self, statement);
        if name.is_some() {
            self.scope.pop();
        }
        self.parent = parent;
    }
    fn visit_body(&mut self, body: &'ast [Stmt]) {
        self.suites.push((
            self.parent.clone().expect("suite parent"),
            body.iter()
                .map(|s| usize::from(s.start())..usize::from(s.end()))
                .collect(),
        ));
        for statement in body {
            self.visit_stmt(statement);
        }
    }
}
impl DefinitionSource {
    /// Parses definitions independently of test-runner naming. Positions refer to
    /// original bytes, including decorators and their complete physical lines.
    /// # Errors
    /// Rejects unsupported encoding, syntax and excessive AST depth.
    pub fn parse(raw: &[u8]) -> Result<Self, String> {
        let decoded = decode_python_source(raw).map_err(|e| e.to_string())?;
        let text = decoded.text();
        if u32::try_from(text.len()).is_err() {
            return Err("Python source exceeds the parser byte-offset domain".into());
        }
        let parsed = super::depth::parse_checked(text, &|| false)
            .map_err(|e| format!("cannot reduce Python definitions: {e:?}"))?;
        let mut collector = Collector::default();
        for statement in &parsed.syntax().body {
            collector.visit_stmt(statement);
        }
        let raw_range = |range: Range<usize>| -> Result<Range<usize>, String> {
            Ok(decoded
                .utf8_to_raw(range.start)
                .ok_or("invalid start boundary")?
                ..decoded
                    .utf8_to_raw(range.end)
                    .ok_or("invalid end boundary")?)
        };
        let definitions = collector
            .definitions
            .into_iter()
            .map(|(name, r)| {
                let mut r = raw_range(r)?;
                r.start = raw[..r.start]
                    .iter()
                    .rposition(|b| *b == b'\n')
                    .map_or(0, |i| i + 1);
                // A BOM belongs to the file, never to its first definition.
                if r.start == 0 && raw.starts_with(b"\xef\xbb\xbf") {
                    r.start = 3;
                }
                r.end = raw[r.end..]
                    .iter()
                    .position(|b| *b == b'\n')
                    .map_or(raw.len(), |i| r.end + i + 1);
                Ok(Definition { name, range: r })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let suites = collector
            .suites
            .into_iter()
            .map(|(parent, body)| {
                Ok((
                    raw_range(parent)?,
                    body.into_iter()
                        .map(raw_range)
                        .collect::<Result<_, String>>()?,
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            raw: raw.to_vec(),
            definitions,
            suites,
        })
    }
    #[must_use]
    pub fn definitions(&self) -> &[Definition] {
        &self.definitions
    }

    /// Selects qualified names using `*` (any sequence) and `?` (one character).
    /// # Errors
    /// Rejects zero matches or any matched name with multiple definitions.
    pub fn select(&self, pattern: &str) -> Result<Vec<usize>, String> {
        let ids: Vec<_> = self
            .definitions
            .iter()
            .enumerate()
            .filter(|(_, d)| name_matches(pattern, &d.name))
            .map(|(i, _)| i)
            .collect();
        if ids.is_empty() {
            return Err(format!("no definition matches {pattern}"));
        }
        let names: BTreeSet<_> = ids.iter().map(|i| &self.definitions[*i].name).collect();
        if names.len() != ids.len() {
            return Err(format!("ambiguous definition selector {pattern}"));
        }
        Ok(ids)
    }

    /// Includes nested definitions physically covered by an ancestor's removal.
    /// # Errors
    /// Rejects IDs outside this source snapshot.
    pub fn covered_ids(&self, ids: &[usize]) -> Result<Vec<usize>, String> {
        let ranges = self.ranges(ids)?;
        Ok(self
            .definitions
            .iter()
            .enumerate()
            .filter(|(_, d)| ranges.iter().any(|r| contains(r, &d.range)))
            .map(|(i, _)| i)
            .collect())
    }
    fn ranges(&self, ids: &[usize]) -> Result<Vec<Range<usize>>, String> {
        ids.iter()
            .map(|i| {
                self.definitions
                    .get(*i)
                    .map(|d| d.range.clone())
                    .ok_or_else(|| "definition ID outside snapshot".into())
            })
            .collect()
    }
    /// Removes whole definitions, repairing only suites that become empty.
    /// Kept ranges are protected even when a removed definition contains them.
    /// # Errors
    /// Rejects unknown IDs, protected overlap or syntactically invalid variants.
    pub fn remove(&self, removed: &[usize], kept: &[usize]) -> Result<Vec<u8>, String> {
        let mut ranges = self.ranges(removed)?;
        let kept = self.ranges(kept)?;
        if ranges
            .iter()
            .any(|r| kept.iter().any(|k| r.start < k.end && k.start < r.end))
        {
            return Err("removal overlaps a kept definition".into());
        }
        ranges.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
        let mut edits: Vec<(Range<usize>, Vec<u8>)> = Vec::new();
        for range in ranges {
            if edits.last().is_some_and(|(r, _)| contains(r, &range)) {
                continue;
            }
            edits.push((range, Vec::new()));
        }
        for (parent, suite) in &self.suites {
            if edits.iter().any(|(range, _)| contains(range, parent)) {
                continue;
            }
            if suite.is_empty()
                || !suite
                    .iter()
                    .all(|s| edits.iter().any(|(r, _)| contains(r, s)))
            {
                continue;
            }
            let first = &suite[0];
            // An enclosing removed definition needs no internal suite repair.
            if let Some((range, replacement)) = edits
                .iter_mut()
                .find(|(r, _)| r.start <= first.start && first.start < r.end)
            {
                let start = range.start;
                let indent = self.raw[start..range.end]
                    .iter()
                    .take_while(|b| matches!(b, b' ' | b'\t' | b'\x0c'))
                    .count();
                replacement.extend_from_slice(&self.raw[start..start + indent]);
                replacement.extend_from_slice(b"pass");
                if self.raw[range.clone()].ends_with(b"\r\n") {
                    replacement.extend_from_slice(b"\r\n");
                } else if self.raw[range.clone()].ends_with(b"\n") {
                    replacement.push(b'\n');
                }
            }
        }
        let mut result = Vec::with_capacity(self.raw.len());
        let mut cursor = 0;
        for (range, replacement) in edits {
            result.extend_from_slice(&self.raw[cursor..range.start]);
            result.extend(replacement);
            cursor = range.end;
        }
        result.extend_from_slice(&self.raw[cursor..]);
        // Parser/depth/codec validation shares the exact original-byte policy.
        Self::parse(&result)?;
        Ok(result)
    }
}
fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

#[must_use]
pub fn name_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<_> = pattern.chars().collect();
    let n: Vec<_> = name.chars().collect();
    let (mut pi, mut ni, mut star, mut retry) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            retry = ni;
        } else if let Some(s) = star {
            retry += 1;
            ni = retry;
            pi = s + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}
