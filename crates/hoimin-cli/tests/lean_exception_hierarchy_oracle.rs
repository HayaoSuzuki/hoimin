#[path = "support/exception_hierarchy_oracle.rs"]
mod corpus;
use corpus::Observation;
use hoimin_cli::cli::parse_config_from as parse_config;

#[tokio::test]
async fn lean_exception_hierarchy_public_plan_correspondence() {
    let mut checked = 0;
    let cases = corpus::selected("strict");
    let expected_count = cases.len();
    let mut mismatches = Vec::new();
    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        corpus::write_sources(&case, dir.path());
        let result = hoimin_cli::plan::create(corpus::config(&case, dir.path())).await;
        let actual = match result {
            Ok(plan) => Observation {
                pairs: plan
                    .manifest
                    .candidates
                    .iter()
                    .map(|c| {
                        (
                            c.candidate.original.clone(),
                            c.candidate.replacement.clone(),
                        )
                    })
                    .collect(),
                error: Some(false),
                truncated: Some(plan.manifest.truncated),
                load: None,
                fingerprint_matches: None,
            },
            Err(error) => {
                assert!(
                    error
                        .to_string()
                        .contains("exception hierarchy summary limit"),
                    "infrastructure-error {}: {error}",
                    case.id
                );
                Observation {
                    pairs: vec![],
                    error: Some(true),
                    truncated: None,
                    load: None,
                    fingerprint_matches: None,
                }
            }
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
    eprintln!("exception hierarchy correspondence: strict={checked}");
}
