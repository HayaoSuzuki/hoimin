#[path = "support/exception_hierarchy_oracle.rs"]
mod corpus;
use corpus::Observation;
use hoimin_cli::cli::parse_config_from as parse_config;

#[tokio::test]
async fn lean_exception_hierarchy_public_plan_correspondence() {
    let mut checked = 0;
    let cases = corpus::selected("strict")
        .into_iter()
        .filter(|case| case.expected.error == Some(false))
        .collect::<Vec<_>>();
    let expected_count = cases.len();
    let mut mismatches = Vec::new();
    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        corpus::write_sources(&case, dir.path());
        let plan = hoimin_cli::plan::create(
            corpus::config(&case, dir.path(), parse_config),
            hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
        )
        .await
        .unwrap();
        let actual = Observation {
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

#[tokio::test]
async fn lean_exception_hierarchy_public_plan_resource_errors() {
    let cases = corpus::selected("strict")
        .into_iter()
        .filter(|case| case.expected.error == Some(true))
        .collect::<Vec<_>>();
    let expected_count = cases.len();
    let mut checked = 0;
    let mut mismatches = Vec::new();
    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        corpus::write_sources(&case, dir.path());

        let result = hoimin_cli::plan::create(
            corpus::config(&case, dir.path(), parse_config),
            hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
        )
        .await;

        let error = result
            .map(|_| ())
            .expect_err("resource-limited plan must fail");
        assert!(
            error
                .to_string()
                .contains("exception hierarchy summary limit"),
            "infrastructure-error {}: {error}",
            case.id
        );
        corpus::compare(
            &case,
            Observation {
                pairs: vec![],
                error: Some(true),
                truncated: None,
                load: None,
                fingerprint_matches: None,
            },
            &mut mismatches,
        );
        checked += 1;
    }
    assert_eq!(checked, expected_count);
    assert!(
        mismatches.is_empty(),
        "semantic mismatches:\n{}",
        mismatches.join("\n")
    );
}
