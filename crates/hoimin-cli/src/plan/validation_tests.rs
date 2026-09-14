use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, MutationCandidate, TargetSlice, validate_candidate};

use super::{PlanError, ValidationStats, candidate_descriptor, validate_requested_descriptors};

fn candidates(path: &str, count: usize, bytes: usize) -> (Vec<u8>, Vec<MutationCandidate>) {
    let mut source = "x = 1 + 2\n".repeat(count);
    if source.len() < bytes {
        source.push('#');
        source.extend(std::iter::repeat_n(' ', bytes - source.len()));
    }
    let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
    let candidates = (0..count)
        .map(|i| {
            let mut candidate = MutationCandidate {
                id: String::new(),
                sequence: i as u64 + 1,
                path: path.into(),
                span: ByteSpan {
                    start: (i * 10 + 6) as u64,
                    length: 1,
                },
                original: "+".into(),
                replacement: "-".into(),
                operator: "binary_add_sub".into(),
                line: u32::try_from(i).unwrap() + 1,
                column: 6,
                symbol: None,
                file_hash: hash.clone(),
            };
            candidate.id = validate_candidate(source.as_bytes(), &candidate_descriptor(&candidate))
                .unwrap()
                .to_string();
            candidate
        })
        .collect();
    (source.into_bytes(), candidates)
}

async fn check(
    project: &super::tests::Project,
    candidates: &[MutationCandidate],
    stats: &mut ValidationStats,
) -> Result<BTreeSet<Utf8PathBuf>, PlanError> {
    let ids = candidates.iter().map(|c| c.id.clone()).collect();
    let map = candidates.iter().map(|c| (c.id.as_str(), c)).collect();
    let targets: Vec<_> = candidates
        .iter()
        .map(|c| TargetSlice {
            path: c.path.clone(),
            lines: vec![],
            symbols: vec![],
        })
        .collect();
    validate_requested_descriptors(&map, &ids, &project.config("30s"), &targets, Some(stats)).await
}

#[tokio::test]
async fn descriptor_validation_cost_tracks_files_and_bytes_not_candidates() {
    for file_count in [1, 2, 4] {
        for count in [1, 2, 4] {
            let project = super::tests::Project::new();
            let mut all = Vec::new();
            for f in 0..file_count {
                let path = format!("src/file{f}.py");
                let (source, mut records) = candidates(&path, count, 4096);
                std::fs::write(project.root.join(&path), source).unwrap();
                all.append(&mut records);
            }
            let mut stats = ValidationStats::default();
            let paths = check(&project, &all, &mut stats).await.unwrap();
            assert_eq!(paths.len(), file_count);
            assert_eq!(stats.contexts, file_count);
            assert_eq!(stats.source_bytes, file_count * 4096);
        }
    }
}

#[tokio::test]
async fn descriptor_validation_preserves_each_validation_and_stable_identity() {
    let project = super::tests::Project::new();
    let (source, original) = candidates("src/calc.py", 1, 0);
    std::fs::write(project.root.join("src/calc.py"), &source).unwrap();
    for (field, message) in [
        ("hash", "candidate file hash does not match the source"),
        ("span", "candidate span is outside the source"),
        (
            "original",
            "candidate original text does not match the source span",
        ),
        (
            "line",
            "candidate line or column does not match its byte span",
        ),
        (
            "column",
            "candidate line or column does not match its byte span",
        ),
        (
            "operator",
            "candidate operator is empty or replacement is unchanged",
        ),
    ] {
        let mut candidate = original[0].clone();
        match field {
            "hash" => candidate.file_hash = "0".repeat(64),
            "span" => candidate.span.start = 100,
            "original" => candidate.original = "?".into(),
            "line" => candidate.line = 2,
            "column" => candidate.column = 7,
            "operator" => candidate.operator.clear(),
            _ => unreachable!(),
        }
        assert_eq!(
            validate_candidate(&source, &candidate_descriptor(&candidate))
                .unwrap_err()
                .to_string(),
            message
        );
        let error = check(&project, &[candidate], &mut ValidationStats::default())
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("plan.candidate.invalid: {message}")
        );
    }
    let mut candidate = original[0].clone();
    candidate.id = format!("m1_{}", "0".repeat(64));
    let error = check(
        &project,
        &[candidate.clone()],
        &mut ValidationStats::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        format!(
            "plan.candidate.invalid: candidate stable id differs for {}",
            candidate.id
        )
    );
    let mut invalid = source;
    invalid.push(255);
    candidate.file_hash = blake3::hash(&invalid).to_hex().to_string();
    std::fs::write(project.root.join("src/calc.py"), invalid).unwrap();
    let error = check(&project, &[candidate], &mut ValidationStats::default())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate source is not valid UTF-8"
    );
}

#[tokio::test]
async fn descriptor_validation_reports_errors_in_requested_id_order() {
    let project = super::tests::Project::new();
    let mut all = Vec::new();
    for path in ["src/a.py", "src/b.py"] {
        let (source, mut records) = candidates(path, 16, 0);
        std::fs::write(project.root.join(path), source).unwrap();
        all.append(&mut records);
    }
    all.sort_by(|a, b| a.id.cmp(&b.id));
    let first_path = all[0].path.clone();
    let other = all.iter().position(|c| c.path != first_path).unwrap();
    let later = all
        .iter()
        .enumerate()
        .skip(other + 1)
        .find(|(_, c)| c.path == first_path)
        .map(|(i, _)| i)
        .unwrap();
    all[other].original = "?".into();
    all[later].file_hash = "0".repeat(64);
    let error = check(&project, &all, &mut ValidationStats::default())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate original text does not match the source span"
    );
}

#[tokio::test]
async fn descriptor_validation_empty_unknown_and_unselected_do_not_read_sources() {
    let project = super::tests::Project::new();
    let config = project.config("30s");
    let mut stats = ValidationStats::default();
    assert!(
        validate_requested_descriptors(
            &BTreeMap::new(),
            &BTreeSet::new(),
            &config,
            &[],
            Some(&mut stats)
        )
        .await
        .unwrap()
        .is_empty()
    );
    let error = validate_requested_descriptors(
        &BTreeMap::new(),
        &BTreeSet::from(["missing".into()]),
        &config,
        &[],
        Some(&mut stats),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate id is not in the plan: missing"
    );
    let (_, records) = candidates("src/absent.py", 1, 0);
    let map = records.iter().map(|c| (c.id.as_str(), c)).collect();
    let ids = records.iter().map(|c| c.id.clone()).collect();
    let error = validate_requested_descriptors(&map, &ids, &config, &[], Some(&mut stats))
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate target is not selected: src/absent.py"
    );
    assert_eq!(stats.contexts, 0);
    assert_eq!(stats.source_bytes, 0);
}

#[tokio::test]
#[ignore = "release validation benchmark; run explicitly with --release --ignored --nocapture"]
async fn benchmark_requested_descriptor_preprocessing() {
    for file_count in [1, 2, 4] {
        for count in [1, 100, 500] {
            let project = super::tests::Project::new();
            let mut all = Vec::new();
            let mut sources = BTreeMap::new();
            for f in 0..file_count {
                let path = format!("src/file{f}.py");
                let (source, mut records) = candidates(&path, count, 2 * 1024 * 1024);
                std::fs::write(project.root.join(&path), &source).unwrap();
                sources.insert(Utf8PathBuf::from(path), source);
                all.append(&mut records);
            }
            for round in 0..3 {
                let start = Instant::now();
                for candidate in &all {
                    let id = validate_candidate(
                        std::hint::black_box(&sources[&candidate.path]),
                        &candidate_descriptor(candidate),
                    )
                    .unwrap();
                    assert_eq!(id.as_str(), candidate.id);
                }
                let legacy_ms = start.elapsed().as_secs_f64() * 1000.0;
                let start = Instant::now();
                let mut stats = ValidationStats::default();
                check(&project, &all, &mut stats).await.unwrap();
                let shared_ms = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(stats.contexts, file_count);
                println!(
                    "files={file_count} candidates_per_file={count} bytes_per_file=2097152 round={round} legacy_ms={legacy_ms:.3} shared_including_read_ms={shared_ms:.3} contexts={} bytes={}",
                    stats.contexts, stats.source_bytes
                );
            }
        }
    }
}
