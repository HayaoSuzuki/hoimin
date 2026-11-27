use super::draft;
use crate::{
    artifacts::run_report::test_support::{current_report_view, jsonl_bytes},
    cli::{ParsedCommand, ReviewArgs, ReviewCommonArgs, ReviewListArgs, parse_from},
    plan::ReadOnlyPlan,
    review::{
        ReviewError, input,
        model::{ArtifactReference, Assessment, SourceState},
        run,
        store::ReviewStore,
    },
};
use hoimin_core::{
    CandidateDescriptor, ExitPolicy, MutationCandidate, MutationOperator, MutationStatus,
    OutputConfig, exit_code_for, summarize,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const FILES: [(&str, &str); 5] = [
    ("flag.py", "flag = True\n"),
    ("first.py", "number = 1\n"),
    ("second.py", "number = 2\n"),
    ("x-retired.py", "number = 3\n"),
    ("y-retired.py", "number = 4\n"),
];
const CURRENT: usize = 3;

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    sources: Vec<Source>,
    stored: Vec<Stored>,
    reported: Option<Vec<Reported>>,
    include_retired: bool,
    expected: Value,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    Current,
    Stale,
    #[serde(rename = "source_unavailable")]
    Unavailable,
}

#[derive(serde::Deserialize)]
struct Stored {
    id: usize,
    descriptor_matches: bool,
    history: Vec<Assessment>,
}

#[derive(serde::Deserialize)]
struct Reported {
    id: usize,
    descriptor_matches: bool,
    status: MutationStatus,
}

struct Project {
    _directory: tempfile::TempDir,
    plan: ReadOnlyPlan,
    pool: Vec<MutationCandidate>,
    report_template: Value,
}

impl Project {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut argv = vec!["hoimin", "plan", "--root", root.to_str().unwrap()];
        for (path, contents) in FILES {
            fs::write(root.join(path), contents).unwrap();
            argv.extend(["--file", path]);
        }
        argv.extend([
            "--operators",
            MutationOperator::BooleanLiteral.as_str(),
            "--operators",
            MutationOperator::IntegerLiteralNeighbor.as_str(),
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
            "--",
            "must-not-execute",
        ]);
        let ParsedCommand::Plan(plan_args) = parse_from(argv).unwrap() else {
            panic!("expected plan");
        };
        let (config, discovery) = plan_args.into_configs().unwrap();
        let mut manifest = crate::plan::create(config, discovery)
            .await
            .unwrap()
            .manifest;
        let mut seen = BTreeSet::new();
        manifest
            .candidates
            .retain(|row| seen.insert(row.path.clone()));
        assert_eq!(
            manifest
                .candidates
                .iter()
                .map(|row| row.path.as_str())
                .collect::<Vec<_>>(),
            FILES.map(|(path, _)| path),
        );
        for (index, row) in manifest.candidates.iter_mut().enumerate() {
            row.rank = index + 1;
            row.candidate.sequence = u64::try_from(index).unwrap() + 1;
        }
        let mut retired = manifest.candidates.split_off(CURRENT);
        retired.sort_by(|left, right| left.id.cmp(&right.id));
        let pool = manifest
            .candidates
            .iter()
            .cloned()
            .chain(retired)
            .map(|row| row.candidate)
            .collect();
        let plan_path = root.join("listing-plan.json");
        fs::write(&plan_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let plan = input::load_plan(&plan_path).unwrap();
        let mut report_template = current_report_view();
        report_template["run"]["normalized_config"] = serde_json::to_value(
            manifest
                .normalized_config
                .into_run_config(OutputConfig::default())
                .unwrap(),
        )
        .unwrap();
        Self {
            _directory: directory,
            plan,
            pool,
            report_template,
        }
    }

    fn write_store(&self, path: &Path, entries: &[Stored]) {
        if entries.is_empty() {
            return;
        }
        let mut store = ReviewStore::open_write(path, &self.plan.project).unwrap();
        for row in entries {
            let mut key = input::review_key(&self.plan.project, &self.pool[row.id]);
            if !row.descriptor_matches {
                key.descriptor.original = "different stored original".into();
            }
            for (index, &assessment) in row.history.iter().enumerate() {
                let mut review = draft();
                review.assessment = assessment;
                review.plan = ArtifactReference {
                    path: self.plan.canonical_path.clone(),
                    content_hash: self.plan.content_hash.clone(),
                };
                store
                    .append(
                        &key,
                        SourceState::Fresh,
                        u64::try_from(index).unwrap(),
                        review,
                    )
                    .unwrap();
            }
        }
    }

    fn write_report(&self, path: &Path, entries: &[Reported]) {
        let mut report = self.report_template.clone();
        let mutants: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let mut mutant = self.report_template["mutants"][0].clone();
                let mut candidate = self.pool[row.id].clone();
                if !row.descriptor_matches {
                    candidate.original = "different reported original".into();
                }
                mutant["candidate"] = serde_json::to_value(candidate).unwrap();
                mutant["sequence"] = json!(4 + 2 * index);
                let exit = match row.status {
                    MutationStatus::Killed => 1,
                    MutationStatus::Survived => 0,
                    _ => unreachable!("the fixture only has conclusive outcomes"),
                };
                mutant["outcome"]["executed"]["termination"] = json!({"Exit":exit});
                mutant["status"] = serde_json::to_value(row.status).unwrap();
                mutant
            })
            .collect();
        report["mutants"] = Value::Array(mutants);
        let counts = summarize(&entries.iter().map(|row| row.status).collect::<Vec<_>>());
        report["summary"]["counts"] = serde_json::to_value(&counts).unwrap();
        report["summary"]["exit_code"] = json!(exit_code_for(ExitPolicy::from_summary(&counts)));
        report["summary"]["sequence"] = json!(3 + 2 * entries.len());
        fs::write(path, jsonl_bytes(&report)).unwrap();
    }

    fn restore_sources(&self) {
        for (name, contents) in FILES.into_iter().take(CURRENT) {
            fs::write(self.plan.project.join(name), contents).unwrap();
        }
    }

    fn set_sources(&self, sources: &[Source]) {
        assert_eq!(sources.len(), CURRENT);
        for ((name, _), source) in FILES.into_iter().zip(sources) {
            let path = self.plan.project.join(name);
            match source {
                Source::Current => {}
                Source::Stale => fs::write(path, b"# changed after the plan\n").unwrap(),
                Source::Unavailable => fs::remove_file(path).unwrap(),
            }
        }
    }
}

fn inspect_rows(rows: &Value, pool: &[MutationCandidate], id: &str) -> Vec<Value> {
    rows.as_array()
        .expect(id)
        .iter()
        .map(|row| {
            let candidate: MutationCandidate =
                serde_json::from_value(row["candidate"].clone()).expect(id);
            let key = pool
                .iter()
                .position(|item| item.id == candidate.id)
                .expect(id);
            assert_eq!(
                CandidateDescriptor::from(&candidate),
                CandidateDescriptor::from(&pool[key]),
                "{id}"
            );
            let history = row["history"].as_array().expect(id);
            let assessments: Vec<_> = history
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    assert_eq!(event["revision"], json!(index + 1), "{id}");
                    &event["draft"]["assessment"]
                })
                .collect();
            if !row["review"].is_null() {
                assert_eq!(&row["review"], history.last().expect(id), "{id}");
            }
            json!({
                "id": key,
                "sequence": candidate.sequence,
                "operator": candidate.operator,
                "applicability": row["applicability"],
                "review": row["review"]["draft"]["assessment"],
                "execution": row["execution"],
                "history": assessments,
            })
        })
        .collect()
}

fn observe(project: &Project, store: PathBuf, report: Option<PathBuf>, case: &Case) -> Value {
    let args = ReviewArgs::List(ReviewListArgs {
        common: ReviewCommonArgs {
            plan: project.plan.canonical_path.clone(),
            store,
            report,
        },
        stale: case.include_retired,
    });
    let id = &case.id;
    let mut stdout = Vec::new();
    match run(&args, &mut stdout) {
        Ok(code) => {
            assert_eq!(code, 0, "{id}");
            let document: Value = serde_json::from_slice(&stdout).expect(id);
            assert_eq!(document["schema_version"], 2, "{id}");
            assert_eq!(document["command"], "list", "{id}");
            json!({
                "kind": "listed",
                "candidates": inspect_rows(&document["candidates"], &project.pool, id),
                "stale_candidates": inspect_rows(&document["stale_candidates"], &project.pool, id),
                "operators": document["operators"],
            })
        }
        Err(ReviewError::Invalid(message)) => {
            assert!(stdout.is_empty(), "{id}");
            let failure = match message.as_str() {
                "stored candidate identity conflict" => "stored_identity",
                "report candidate descriptor differs from stored candidate" => "report_descriptor",
                _ => panic!("{id}: unexpected invalid input {message:?}"),
            };
            json!({"kind":"error", "failure":failure})
        }
        Err(error) => panic!("{id}: unexpected error {error:?}"),
    }
}

#[tokio::test]
async fn review_listing_matches_lean_with_real_store_and_reports() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/review-listing.jsonl"
    ));
    let project = Project::new().await;
    let mut ids = BTreeSet::new();
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        let id = &case.id;
        assert_eq!(case.schema, 1, "{id}");
        assert_eq!(case.mode, "internal-fixture", "{id}");
        assert!(ids.insert(id.clone()), "{id}");
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("reviews.sqlite3");
        project.restore_sources();
        project.write_store(&store, &case.stored);
        let report = case.reported.as_ref().map(|entries| {
            let path = directory.path().join("run.jsonl");
            project.write_report(&path, entries);
            path
        });
        project.set_sources(&case.sources);

        let actual = observe(&project, store, report, &case);

        assert_eq!(actual, case.expected, "{id}");
    }
    assert_eq!(ids.len(), 102);
}
