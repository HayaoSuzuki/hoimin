use super::report_fixture;
use crate::{
    artifacts::{
        READ_ONLY_ARTIFACT_LIMIT,
        run_report::test_support::{jsonl_bytes, report_view},
    },
    review::{JOIN_WORK, JoinWork, ReviewError, input},
};
use hoimin_core::{
    CandidateDescriptor, CandidateIdentity, ExitPolicy, MutationCandidate, MutationStatus,
    exit_code_for, stable_mutant_id, summarize,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    artifact: Artifact,
    summary_matches: bool,
    identities_match: bool,
    project: Project,
    baseline: Baseline,
    complete: bool,
    candidates: Vec<Row>,
    plan_probes: usize,
    outcome: Outcome,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Artifact {
    Missing,
    Oversized,
    InvalidJson,
    InvalidConfig,
    WrongSchema,
    Jsonl,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Project {
    Same,
    Alias,
    Other,
    Missing,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Baseline {
    Passed,
    Failed,
    Missing,
}

#[derive(serde::Deserialize)]
struct Row {
    id: usize,
    original_matches: bool,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Failure {
    Read,
    Size,
    Parse,
    Schema,
    Summary,
    Identity,
    ProjectResolution,
    Unusable,
    ProjectMismatch,
    Descriptor,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Rejected { failure: Failure },
    Admitted { candidate_ids: Vec<usize> },
}

struct Roots {
    project: PathBuf,
    alias: PathBuf,
    other: PathBuf,
    missing: PathBuf,
}

impl Roots {
    fn new(project: &Path) -> Self {
        let child = project.join("alias-container");
        let other = project.join("different-project");
        fs::create_dir(&child).unwrap();
        fs::create_dir(&other).unwrap();
        Self {
            project: project.to_owned(),
            alias: child.join(".."),
            other,
            missing: project.join("missing-project"),
        }
    }

    fn select(&self, project: &Project) -> &Path {
        match project {
            Project::Same => &self.project,
            Project::Alias => &self.alias,
            Project::Other => &self.other,
            Project::Missing => &self.missing,
        }
    }
}

fn extra_candidate(template: &MutationCandidate, replacement: &str) -> MutationCandidate {
    let mut candidate = template.clone();
    candidate.replacement = replacement.into();
    candidate.id = stable_mutant_id(&CandidateIdentity::from(&CandidateDescriptor::from(
        &candidate,
    )))
    .to_string();
    candidate
}

fn render_report(
    case: &Case,
    template: &Value,
    roots: &Roots,
    pool: &[MutationCandidate; 3],
) -> Vec<u8> {
    let mut report = template.clone();
    report["run"]["normalized_config"]["selection"]["root"] = json!(roots.select(&case.project));
    match case.baseline {
        Baseline::Passed => {}
        Baseline::Failed => report["baseline"]["termination"] = json!({"Exit": 1}),
        Baseline::Missing => report["baseline"] = Value::Null,
    }
    let mut mutants: Vec<_> = case
        .candidates
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let mut candidate = pool[row.id].clone();
            candidate.sequence = u64::try_from(index).unwrap() + 1;
            if !row.original_matches {
                candidate.original = "different original text".into();
            }
            let mut mutant = template["mutants"][0].clone();
            mutant["candidate"] = serde_json::to_value(candidate).unwrap();
            mutant["sequence"] = json!(4 + 2 * index);
            mutant
        })
        .collect();
    if !case.identities_match {
        mutants[0]["candidate"]["replacement"] = json!("forged replacement");
    }
    let count = mutants.len();
    report["mutants"] = Value::Array(mutants);
    let counts = summarize(&vec![MutationStatus::Killed; count]);
    let summary = &mut report["summary"];
    summary["sequence"] = json!(3 + 2 * count);
    summary["counts"] = serde_json::to_value(&counts).unwrap();
    if !case.summary_matches {
        summary["counts"]["killed"] = json!(count + 1);
    }
    summary["complete"] = json!(case.complete);
    summary["exit_code"] = json!(if case.complete {
        exit_code_for(ExitPolicy::from_summary(&counts))
    } else {
        4
    });
    match case.artifact {
        Artifact::InvalidConfig => report["run"]["normalized_config"] = Value::Null,
        Artifact::WrongSchema => report["run"]["schema_version"] = json!(0),
        _ => {}
    }
    jsonl_bytes(&report)
}

fn write_report(
    case: &Case,
    path: &Path,
    template: &Value,
    roots: &Roots,
    pool: &[MutationCandidate; 3],
) {
    match case.artifact {
        Artifact::Missing => {}
        Artifact::Oversized => fs::File::create(path)
            .unwrap()
            .set_len(READ_ONLY_ARTIFACT_LIMIT + 1)
            .unwrap(),
        Artifact::InvalidJson => fs::write(path, b"not a JSONL event\n").unwrap(),
        Artifact::InvalidConfig | Artifact::WrongSchema | Artifact::Jsonl => {
            fs::write(path, render_report(case, template, roots, pool)).unwrap();
        }
    }
}

fn invalid_reason(message: &str, path: &Path, id: &str) -> Failure {
    let path = path.display();
    match message {
        "review requires a complete report with a passing baseline" => Failure::Unusable,
        "report belongs to another project" => Failure::ProjectMismatch,
        "report candidate descriptor differs from plan" => Failure::Descriptor,
        _ if message == format!("input exceeds size limit or is not a file: {path}") => {
            Failure::Size
        }
        _ if message.starts_with(&format!("could not parse report {path}: ")) => Failure::Parse,
        _ if message.starts_with(&format!("unsupported schema version 0 in report {path};")) => {
            Failure::Schema
        }
        _ if message
            == format!(
                "invalid structure in report {path}: summary counts must match mutant events"
            ) =>
        {
            Failure::Summary
        }
        _ if message
            == format!(
                "invalid structure in report {path}: candidate identity conflict: stable ID does not match identity fields"
            ) =>
        {
            Failure::Identity
        }
        _ => panic!("{id}: unexpected invalid input {message:?}"),
    }
}

fn observe(
    case: &Case,
    report_path: &Path,
    plan: &crate::plan::ReadOnlyPlan,
    roots: &Roots,
    template: &Value,
    pool: &[MutationCandidate; 3],
) -> Outcome {
    match input::load_report(report_path, plan, &input::index_plan(plan)) {
        Ok((report, reference)) => {
            let id = &case.id;
            assert_eq!(report.project, plan.project, "{id}");
            assert_eq!(
                Some(report.run_id.as_str()),
                template["run"]["run_id"].as_str(),
                "{id}"
            );
            assert_eq!(reference.run_id, report.run_id, "{id}");
            assert_eq!(
                reference.artifact.path,
                fs::canonicalize(report_path).unwrap(),
                "{id}"
            );
            let fingerprint = blake3::hash(&fs::read(report_path).unwrap()).to_hex();
            assert_eq!(
                reference.artifact.content_hash,
                fingerprint.as_str(),
                "{id}"
            );
            let candidate_ids = report
                .mutants
                .iter()
                .map(|mutant| {
                    pool.iter()
                        .position(|candidate| candidate.id == mutant.candidate.id)
                        .expect(id)
                })
                .collect();
            Outcome::Admitted { candidate_ids }
        }
        Err(error) => {
            let failure = match error {
                ReviewError::Io(error) if error.kind() == ErrorKind::NotFound => Failure::Read,
                ReviewError::ReportProject {
                    report,
                    project,
                    source,
                } => {
                    assert_eq!(report, report_path, "{}", case.id);
                    assert_eq!(project, roots.missing, "{}", case.id);
                    assert_eq!(source.kind(), ErrorKind::NotFound, "{}", case.id);
                    Failure::ProjectResolution
                }
                ReviewError::Invalid(message) => invalid_reason(&message, report_path, &case.id),
                error => panic!("{}: unexpected error {error:?}", case.id),
            };
            Outcome::Rejected { failure }
        }
    }
}

#[tokio::test]
async fn review_report_admission_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/review-report-admission.jsonl"
    ));
    let (directory, plan_path, base_path) = report_fixture().await;
    let mut plan = input::load_plan(&plan_path).unwrap();
    plan.manifest.candidates.truncate(1);
    let roots = Roots::new(directory.path());
    assert_ne!(roots.alias, roots.project);
    assert_eq!(
        fs::canonicalize(&roots.alias).unwrap(),
        plan.project.as_std_path()
    );
    let template = report_view(&fs::read(base_path).unwrap());
    let known = &plan.manifest.candidates[0].candidate;
    let pool = [
        known.clone(),
        extra_candidate(known, "12345"),
        extra_candidate(known, "67890"),
    ];
    let mut ids = BTreeSet::new();
    for (index, line) in corpus.lines().enumerate() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        let report_path = directory.path().join(format!("case-{index}.jsonl"));
        write_report(&case, &report_path, &template, &roots, &pool);
        JOIN_WORK.with_borrow_mut(|work| *work = JoinWork::default());

        let actual = observe(&case, &report_path, &plan, &roots, &template, &pool);

        let probes = JOIN_WORK.with_borrow(|work| work.plan_probes);
        assert_eq!(probes, case.plan_probes, "{}", case.id);
        assert_eq!(actual, case.outcome, "{}", case.id);
        if !matches!(case.artifact, Artifact::Missing) {
            fs::remove_file(report_path).unwrap();
        }
    }
    assert_eq!(ids.len(), 97);
}
