use super::super::rust::exception_hierarchy::oracle_corpus as corpus;
use super::ExceptionProject;
use crate::cli::parse_config_from as parse_config;
use camino::Utf8Path;
use corpus::Observation;

fn replay_snapshot_actions(
    case: &corpus::Case,
    project: &ExceptionProject,
    path: &std::path::Path,
    original: &str,
) -> (
    Option<std::sync::Arc<super::super::rust::exception_hierarchy::ExceptionIndex>>,
    &'static str,
    Vec<String>,
) {
    let mut errors = Vec::new();
    let mut cached = None;
    let mut load = "never";
    for action in &case.actions {
        match action.as_str() {
            "change" => std::fs::write(path, &case.changed_source).unwrap(),
            "restore" => std::fs::write(path, original).unwrap(),
            "delete" => match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("infrastructure-error {}: {error}", case.id),
            },
            "build" => match project.load(&|| false) {
                Ok(index) => {
                    cached = Some(index);
                    load = "ok";
                }
                Err(error) => {
                    errors.push(error);
                    load = "error";
                }
            },
            _ => unreachable!(),
        }
    }
    (cached, load, errors)
}

#[test]
fn lean_exception_hierarchy_snapshot_correspondence() {
    let mut checked = 0;
    let cases: Vec<_> = corpus::selected("internal-fixture")
        .into_iter()
        .filter(|case| case.kind == "snapshot")
        .collect();
    assert!(!cases.is_empty(), "no snapshot cases selected");
    let expected_count = cases.len();
    let mut mismatches = Vec::new();
    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        corpus::write_sources(&case, dir.path());
        let config =
            crate::shell::prepare_run_config(corpus::config(&case, dir.path(), parse_config))
                .unwrap();
        let project = ExceptionProject::from_config(&config).unwrap();
        let original = &case.files.iter().find(|(p, _)| p == "errors.py").unwrap().1;
        let service = &case
            .files
            .iter()
            .find(|(p, _)| p == "service.py")
            .unwrap()
            .1;
        let path = dir.path().join("errors.py");
        let (cached, load, errors) = replay_snapshot_actions(&case, &project, &path, original);
        assert!(
            errors
                .iter()
                .all(|error| error.contains("input changed after fingerprint preparation")),
            "infrastructure-error {}: {errors:?}",
            case.id
        );
        let pairs = cached
            .map(|index| {
                let mut pairs = Vec::new();
                let parsed = ruff_python_parser::parse_module(service).unwrap();
                index
                    .collect(
                        Utf8Path::new("service.py"),
                        parsed.syntax(),
                        case.max_candidates,
                        &|| false,
                        |range, replacement| pairs.push((service[range].to_owned(), replacement)),
                    )
                    .unwrap();
                pairs
            })
            .unwrap_or_default();
        let recheck = crate::fingerprint_inputs::recheck_config(&config, &config.selection.root);
        let fingerprint_matches = recheck.is_ok();
        let expected_recheck = [
            Err(
                crate::fingerprint_inputs::FingerprintInputRecheckError::RecordsChanged.to_string(),
            ),
            Ok(()),
        ]
        .into_iter()
        .nth(usize::from(case.expected.fingerprint_matches.unwrap()))
        .unwrap();

        assert_eq!(
            recheck.map_err(|error| error.to_string()),
            expected_recheck,
            "{}",
            case.id
        );
        let actual = Observation {
            pairs,
            error: None,
            truncated: None,
            load: Some(load.into()),
            fingerprint_matches: Some(fingerprint_matches),
        };
        corpus::compare(&case, actual, &mut mismatches);
        checked += 1;
    }
    assert_eq!(checked, expected_count);
    assert!(
        mismatches.is_empty(),
        "semantic mismatches:\n{}",
        mismatches.join("\n")
    );
    eprintln!("exception hierarchy correspondence: internal-fixture={checked}");
}
