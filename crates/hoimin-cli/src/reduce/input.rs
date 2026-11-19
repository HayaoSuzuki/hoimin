use super::ReduceArgs;
pub(super) type Files = Vec<(Utf8PathBuf, Vec<u8>)>;
type Variant = (Files, Vec<Span>);
use camino::Utf8PathBuf;
use hoimin_core::reduce::Span;
use hoimin_core::{MutationCandidate, RunConfig};
use hoimin_python::reduce::DefinitionSource;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(super) struct Source {
    pub path: Utf8PathBuf,
    pub bytes: Vec<u8>,
    pub definitions: DefinitionSource,
    pub first_id: u64,
    pub offset: u64,
}
pub(super) struct Input {
    pub config: RunConfig,
    pub candidates: Vec<MutationCandidate>,
    pub sources: Vec<Source>,
    pub selected: Vec<u64>,
    pub kept: Vec<u64>,
    pub original: Vec<u64>,
    pub kept_spans: Vec<Span>,
    pub plan_hash: String,
    pub rate: (u128, u128),
    pub output: PathBuf,
}

pub(super) fn rate(value: &str) -> Result<(u128, u128), String> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(
            "min-reduction must be a decimal percentage with at most six decimal places".into(),
        );
    }
    let scale = 10u128.pow(u32::try_from(fraction.len()).map_err(|e| e.to_string())?);
    let numerator = whole
        .parse::<u128>()
        .ok()
        .and_then(|v| v.checked_mul(scale))
        .and_then(|v| v.checked_add(fraction.parse::<u128>().unwrap_or(0)))
        .ok_or("invalid min-reduction")?;
    let denominator = 100 * scale;
    if !hoimin_core::reduce::valid_rate(numerator, denominator) {
        return Err("min-reduction must be strictly between 0 and 100".into());
    }
    Ok((numerator, denominator))
}

impl Input {
    #[allow(
        clippy::too_many_lines,
        reason = "one preflight transaction validates selectors against the same immutable plan and source snapshots"
    )]
    pub async fn prepare(args: &ReduceArgs) -> Result<Self, String> {
        let rate = rate(&args.min_reduction)?;
        let verified = crate::plan::prepare_verify(&args.plan, &args.candidate)
            .await
            .map_err(|e| e.to_string())?;
        let plan = crate::plan::read_for_review(&args.plan).map_err(|e| e.to_string())?;
        let config = verified.config().clone();
        let ids: BTreeSet<_> = args.candidate.iter().collect();
        let candidates = plan
            .manifest
            .candidates
            .into_iter()
            .filter(|c| ids.contains(&c.id))
            .map(|c| c.candidate)
            .collect();
        let mut patterns: BTreeMap<Utf8PathBuf, (Vec<String>, Vec<String>)> = BTreeMap::new();
        for (selectors, keep) in [(&args.test, false), (&args.keep, true)] {
            for selector in selectors {
                let (file, pattern) = selector
                    .rsplit_once(':')
                    .ok_or("expected PATH:QUALNAME_GLOB selector")?;
                let path = Utf8PathBuf::from(file);
                if path.is_absolute()
                    || path
                        .components()
                        .any(|c| !matches!(c, camino::Utf8Component::Normal(_)))
                    || file.chars().any(char::is_control)
                    || pattern.is_empty()
                {
                    return Err(format!("invalid definition selector: {selector}"));
                }
                let entry = patterns.entry(path).or_default();
                if keep {
                    entry.1.push(pattern.into());
                } else {
                    entry.0.push(pattern.into());
                }
            }
        }
        let reader = crate::source_file::PortableFileReader::open(&plan.project)
            .map_err(|e| e.to_string())?;
        let mut sources = Vec::new();
        let mut selected = BTreeSet::new();
        let mut kept = BTreeSet::new();
        let mut original = Vec::new();
        let mut kept_spans = Vec::new();
        let mut offset = 0;
        for (path, (tests, keeps)) in patterns {
            if plan.manifest.sources.iter().any(|s| s.path == path)
                || config.fingerprint_inputs.iter().any(|s| s.path == path)
            {
                return Err(format!(
                    "{path}: reduction target overlaps mutation source or explicit fingerprint input"
                ));
            }
            let full = plan.project.join(&path);
            let canonical = std::fs::canonicalize(&full).map_err(|e| e.to_string())?;
            if canonical != full.as_std_path() {
                return Err(format!(
                    "{path}: reduction input must not traverse symlinks"
                ));
            }
            let bytes = reader
                .read_bounded(
                    &path,
                    usize::try_from(config.limits.max_copy_size.get().min(u64::MAX - 1))
                        .unwrap_or(usize::MAX),
                )
                .map_err(|e| e.to_string())?;
            let definitions =
                DefinitionSource::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
            let first_id = u64::try_from(original.len()).map_err(|e| e.to_string())?;
            original.extend(
                (0..definitions.definitions().len())
                    .map(|i| first_id + u64::try_from(i).expect("definition index")),
            );
            for pattern in tests {
                selected.extend(
                    definitions
                        .select(&pattern)?
                        .into_iter()
                        .map(|i| first_id + u64::try_from(i).expect("definition index")),
                );
            }
            for pattern in keeps {
                for i in definitions.select(&pattern)? {
                    let id = first_id + u64::try_from(i).map_err(|e| e.to_string())?;
                    kept.insert(id);
                    let range = &definitions.definitions()[i].range;
                    kept_spans.push(Span {
                        id,
                        lo: offset + u64::try_from(range.start).map_err(|e| e.to_string())?,
                        hi: offset + u64::try_from(range.end).map_err(|e| e.to_string())?,
                    });
                }
            }
            let next_offset = offset
                .checked_add(u64::try_from(bytes.len()).map_err(|e| e.to_string())?)
                .and_then(|o| o.checked_add(1))
                .ok_or("source span overflow")?;
            sources.push(Source {
                path,
                bytes,
                definitions,
                first_id,
                offset,
            });
            offset = next_offset;
        }
        selected.retain(|id| !kept.contains(id));
        // Protect a kept descendant as well as direct keep/target overlap.
        selected.retain(|id| {
            let source = sources
                .iter()
                .rev()
                .find(|s| s.first_id <= *id)
                .expect("source for ID");
            let range = &source.definitions.definitions()
                [usize::try_from(*id - source.first_id).expect("index")]
            .range;
            let span = Span {
                id: *id,
                lo: source.offset + range.start as u64,
                hi: source.offset + range.end as u64,
            };
            !kept_spans.iter().any(|k| span.overlaps(k))
        });
        let parent = args
            .output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        let parent = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
        let name = args
            .output
            .file_name()
            .ok_or("output requires a new directory name")?;
        let output = parent.join(name);
        if output.starts_with(plan.project.as_std_path()) || output.symlink_metadata().is_ok() {
            return Err("output must be a new directory outside the project".into());
        }
        Ok(Self {
            config,
            candidates,
            sources,
            selected: selected.into_iter().collect(),
            kept: kept.into_iter().collect(),
            original,
            kept_spans,
            plan_hash: plan.content_hash,
            rate,
            output,
        })
    }

    pub fn verify_sources(&self) -> Result<(), String> {
        let reader = crate::source_file::PortableFileReader::open(&self.config.selection.root)
            .map_err(|e| e.to_string())?;
        for source in &self.sources {
            let current = reader
                .read_bounded(&source.path, source.bytes.len())
                .map_err(|e| e.to_string())?;
            if current != source.bytes {
                return Err(format!("{}: original test source changed", source.path));
            }
        }
        Ok(())
    }

    pub fn variant(&self, removed: &[u64]) -> Result<Variant, String> {
        let mut files = Vec::new();
        let mut spans = Vec::new();
        for source in &self.sources {
            let count = source.definitions.definitions().len() as u64;
            let local = |ids: &[u64]| {
                ids.iter()
                    .filter(|id| **id >= source.first_id && **id < source.first_id + count)
                    .map(|id| {
                        usize::try_from(*id - source.first_id)
                            .expect("index is bounded by allocated definition count")
                    })
                    .collect::<Vec<_>>()
            };
            let ids = local(removed);
            let kept = local(&self.kept);
            for i in source.definitions.covered_ids(&ids)? {
                let r = &source.definitions.definitions()[i].range;
                spans.push(Span {
                    id: source.first_id + i as u64,
                    lo: source.offset + r.start as u64,
                    hi: source.offset + r.end as u64,
                });
            }
            files.push((source.path.clone(), source.definitions.remove(&ids, &kept)?));
        }
        Ok((files, spans))
    }
}
