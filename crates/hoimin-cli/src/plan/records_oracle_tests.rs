use std::{collections::BTreeSet, fs, time::Duration};

use hoimin_core::{DiscoveryConfig, FingerprintInputFile, RunConfig};

use super::{
    PlanError, PlanManifest, RecordMismatch, create, ensure_exact_records, prepare_verify,
    tests::Project, validate_header,
};

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    id: String,
    live: bool,
    saved: Inventory,
    current: Inventory,
    expected: Expected,
}

#[derive(serde::Deserialize)]
struct Inventory {
    sources: Vec<FingerprintInputFile>,
    inputs: Vec<FingerprintInputFile>,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Source,
    FingerprintInput,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Invalid {
    Path,
    Hash,
    Duplicate,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ChangeKind {
    Added,
    Removed,
    Modified,
}

#[derive(serde::Deserialize)]
struct Change {
    path: String,
    kind: ChangeKind,
}

#[derive(serde::Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
enum Expected {
    Ok,
    Invalid {
        kind: Kind,
        issue: Invalid,
        path: String,
    },
    Stale {
        kind: Kind,
        displayed: Vec<Change>,
        omitted: usize,
    },
}

impl Expected {
    fn diagnostic(&self) -> Result<(), String> {
        match self {
            Self::Ok => Ok(()),
            Self::Invalid { kind, issue, path } => {
                let field = match kind {
                    Kind::Source => "sources",
                    Kind::FingerprintInput => "normalized_config.fingerprint_inputs",
                };
                let reason = match issue {
                    Invalid::Path => "path is not root-relative",
                    Invalid::Hash => "hash is not a BLAKE3 hex digest",
                    Invalid::Duplicate => "contains duplicate path",
                };
                Err(format!("plan.manifest.invalid: {field} {reason}: {path}"))
            }
            Self::Stale {
                kind,
                displayed,
                omitted,
            } => {
                let (code, description) = match kind {
                    Kind::Source => ("source", "target source"),
                    Kind::FingerprintInput => ("fingerprint_input", "fingerprint input"),
                };
                let mut details: Vec<_> = displayed
                    .iter()
                    .map(|change| {
                        let action = match change.kind {
                            ChangeKind::Added => "added",
                            ChangeKind::Removed => "removed",
                            ChangeKind::Modified => "modified",
                        };
                        format!("{action} {:?}", change.path)
                    })
                    .collect();
                if *omitted != 0 {
                    details.push(format!("{omitted} additional paths omitted"));
                }
                Err(format!(
                    "plan.{code}.changed: planned {description} records do not match the current workspace: {}",
                    details.join("; ")
                ))
            }
        }
    }
}

fn check_records(manifest: &PlanManifest, current: &Inventory) -> Result<(), PlanError> {
    validate_header(manifest)?;
    ensure_exact_records(&manifest.sources, &current.sources, RecordMismatch::Source)?;
    ensure_exact_records(
        &manifest.normalized_config.fingerprint_inputs,
        &current.inputs,
        RecordMismatch::FingerprintInput,
    )
}

fn use_workspace_digests(
    saved: &mut [FingerprintInputFile],
    tokens: &[FingerprintInputFile],
    observed: &[FingerprintInputFile],
    case: &str,
) {
    assert_eq!(tokens.len(), observed.len(), "{case}");
    for (token, file) in tokens.iter().zip(observed) {
        assert_eq!(token.path, file.path, "{case}");
        for row in saved.iter_mut().filter(|row| row.hash == token.hash) {
            row.hash.clone_from(&file.hash);
        }
    }
}

async fn seed_plan(project: &Project) -> PlanManifest {
    fs::write(project.root.join("support.cfg"), "live configuration\n").unwrap();
    let mut data = project.config().into_data();
    data.fingerprint_files.push("support.cfg".into());
    create(
        RunConfig::try_from(data).unwrap(),
        DiscoveryConfig::new(Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap()
    .manifest
}

#[tokio::test]
async fn saved_plan_records_match_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/plan-records.jsonl"
    ));
    let project = Project::new();
    let base = seed_plan(&project).await;
    let requested = [base.candidates[0].id.clone()];
    let artifact = tempfile::NamedTempFile::new().unwrap();
    let mut ids = BTreeSet::new();
    let mut live = 0;
    for line in corpus.lines() {
        let mut case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        if case.live {
            live += 1;
            use_workspace_digests(
                &mut case.saved.sources,
                &case.current.sources,
                &base.sources,
                &case.id,
            );
            use_workspace_digests(
                &mut case.saved.inputs,
                &case.current.inputs,
                &base.normalized_config.fingerprint_inputs,
                &case.id,
            );
        }

        let mut manifest = base.clone();
        manifest.sources = case.saved.sources;
        manifest.normalized_config.fingerprint_inputs = case.saved.inputs;
        let result = if case.live {
            let saved = serde_json::to_vec(&manifest).unwrap();
            fs::write(artifact.path(), &saved).unwrap();
            let result = prepare_verify(artifact.path(), &requested)
                .await
                .map(|_| ());
            assert_eq!(fs::read(artifact.path()).unwrap(), saved, "{}", case.id);
            result
        } else {
            check_records(&manifest, &case.current)
        };
        assert_eq!(
            result.map_err(|error| error.to_string()),
            case.expected.diagnostic(),
            "{}",
            case.id
        );
    }
    assert_eq!(ids.len(), 201);
    assert_eq!(live, 9);
}
