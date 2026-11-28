use super::plan_fixture;
use crate::review::{ReviewError, input};
use camino::Utf8PathBuf;
use hoimin_core::FingerprintInputFile;
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
    unix_only: bool,
    placements: Vec<Placement>,
    report_path: ReportPath,
    source_missing: bool,
    fingerprint_link: Option<String>,
    expected: Outcome,
}

#[derive(serde::Deserialize)]
struct Placement {
    suffix: String,
    #[serde(flatten)]
    kind: PlacementKind,
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PlacementKind {
    Regular,
    Directory,
    HardLink { input: Protected },
    CopyPlan,
    SymlinkPlan,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Protected {
    Plan,
    Report,
    Source,
    Fingerprint,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportPath {
    Present,
    Omitted,
    Missing,
    Directory,
    IoError,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Allowed,
    DestinationKind,
    AliasesInput,
    IoError,
}

struct Inputs {
    plan: PathBuf,
    report: PathBuf,
    source: PathBuf,
    fingerprint: PathBuf,
}

impl Inputs {
    fn prepare(case: &Case, plan: &crate::plan::ReadOnlyPlan, source: &[u8]) -> Self {
        let root = plan.project.as_std_path();
        let mut report = root.join("report.jsonl");
        match case.report_path {
            ReportPath::Present | ReportPath::Omitted => {
                fs::write(&report, b"report contents").unwrap();
            }
            ReportPath::Missing => {}
            ReportPath::Directory => fs::create_dir(&report).unwrap(),
            ReportPath::IoError => {
                let parent = root.join("regular-report-parent");
                fs::write(&parent, b"regular file").unwrap();
                report = parent.join("report.jsonl");
            }
        }
        let source_path = root.join("sample.py");
        if !case.source_missing {
            fs::write(&source_path, source).unwrap();
        }
        let fingerprint = root.join("settings.toml");
        if case.fingerprint_link.is_none() {
            fs::write(&fingerprint, b"flag = true\n").unwrap();
        }
        Self {
            plan: plan.canonical_path.clone(),
            report,
            source: source_path,
            fingerprint,
        }
    }

    fn get(&self, input: &Protected) -> &Path {
        match input {
            Protected::Plan => &self.plan,
            Protected::Report => &self.report,
            Protected::Source => &self.source,
            Protected::Fingerprint => &self.fingerprint,
        }
    }

    fn paths(&self) -> [&Path; 4] {
        [&self.plan, &self.report, &self.source, &self.fingerprint]
    }
}

fn destination(root: &Path, suffix: &str) -> PathBuf {
    assert!(matches!(suffix, "" | "-journal" | "-wal" | "-shm"));
    root.join(format!("reviews.sqlite3{suffix}"))
}

#[cfg(unix)]
fn symlink(target: &Path, path: &Path) {
    std::os::unix::fs::symlink(target, path).unwrap();
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _path: &Path) {
    unreachable!("symlink cases are restricted to Unix");
}

fn place_destinations(case: &Case, root: &Path, inputs: &Inputs) {
    for placement in &case.placements {
        let path = destination(root, &placement.suffix);
        match &placement.kind {
            PlacementKind::Regular => fs::write(path, b"distinct file").unwrap(),
            PlacementKind::Directory => fs::create_dir(path).unwrap(),
            PlacementKind::HardLink { input } => fs::hard_link(inputs.get(input), path).unwrap(),
            PlacementKind::CopyPlan => {
                fs::copy(&inputs.plan, path).unwrap();
            }
            PlacementKind::SymlinkPlan => symlink(&inputs.plan, &path),
        }
    }
    if let Some(suffix) = &case.fingerprint_link {
        symlink(&destination(root, suffix), &inputs.fingerprint);
    }
}

fn observed(result: Result<(), ReviewError>, id: &str) -> Outcome {
    match result {
        Ok(()) => Outcome::Allowed,
        Err(ReviewError::Invalid(message)) => match message.as_str() {
            "store aliases a protected input" => Outcome::AliasesInput,
            "store must be a regular file, not a symlink" => Outcome::DestinationKind,
            _ => panic!("{id}: unexpected invalid input {message:?}"),
        },
        Err(ReviewError::Io(error)) if error.kind() == ErrorKind::NotADirectory => Outcome::IoError,
        Err(error) => panic!("{id}: unexpected review error {error:?}"),
    }
}

fn contents(path: &Path) -> Option<Vec<u8>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Some(fs::read(path).unwrap()),
        Ok(_) => None,
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            None
        }
        Err(error) => panic!("cannot inspect fixture {path:?}: {error}"),
    }
}

#[tokio::test]
async fn review_store_destination_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/review-store-destination.jsonl"
    ));
    let (_fixture, plan_path) = plan_fixture().await;
    let mut plan = input::load_plan(&plan_path).unwrap();
    let source = fs::read(plan.project.join("sample.py")).unwrap();
    plan.manifest.normalized_config.fingerprint_inputs = vec![FingerprintInputFile {
        path: "settings.toml".into(),
        hash: blake3::hash(b"flag = true\n").to_hex().to_string(),
    }];
    let mut ids = BTreeSet::new();
    let mut matched = 0;
    for row in corpus.lines() {
        let case: Case = serde_json::from_str(row).unwrap();
        let id = &case.id;
        assert_eq!(case.schema, 1, "{id}");
        assert_eq!(case.mode, "internal-fixture", "{id}");
        assert!(ids.insert(id.clone()), "{id}");
        if case.unix_only && !cfg!(unix) {
            continue;
        }
        let directory = tempfile::tempdir().unwrap();
        plan.project =
            Utf8PathBuf::from_path_buf(fs::canonicalize(directory.path()).unwrap()).unwrap();
        let root = plan.project.as_std_path();
        let inputs = Inputs::prepare(&case, &plan, &source);
        place_destinations(&case, root, &inputs);
        let destinations = ["", "-journal", "-wal", "-shm"].map(|suffix| destination(root, suffix));
        let paths = inputs
            .paths()
            .into_iter()
            .chain(destinations.iter().map(PathBuf::as_path));
        let before: Vec<_> = paths.clone().map(contents).collect();
        let report =
            (!matches!(case.report_path, ReportPath::Omitted)).then_some(inputs.report.as_path());

        let actual = input::validate_store_destination(&destinations[0], &plan, report);

        assert_eq!(observed(actual, id), case.expected, "{id}");
        assert_eq!(paths.map(contents).collect::<Vec<_>>(), before, "{id}");
        matched += 1;
    }
    assert_eq!(ids.len(), 59);
    assert_eq!(matched, if cfg!(unix) { 59 } else { 46 });
}
