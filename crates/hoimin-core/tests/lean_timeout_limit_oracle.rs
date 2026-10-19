use std::collections::BTreeSet;
use std::time::Duration;

use camino::Utf8PathBuf;
use hoimin_budget_math::{MutantTimeout, auto_mutant_timeout};
use hoimin_core::{CommandArg, ConfigError, MAX_TIMEOUT, PlanConfig, RawRunConfig, RunConfig};
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/timeout-limit.jsonl");
const NANOS_PER_SECOND: u64 = 1_000_000_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    analyzer_ns: String,
    baseline_ns: String,
    mutant_mode: String,
    fixed_mutant_ns: Option<String>,
    total_ns: String,
    maximum_ns: String,
    expected_accepted: bool,
    expected_invalid_field: Option<String>,
    expected_effective_mutant_ns: String,
}

fn parse_duration(id: &str, field: &str, value: &str) -> Result<Duration, String> {
    let nanoseconds = value
        .parse::<u64>()
        .map_err(|error| format!("{id} has invalid {field}: {error}"))?;
    Ok(Duration::new(
        nanoseconds / NANOS_PER_SECOND,
        u32::try_from(nanoseconds % NANOS_PER_SECOND).expect("nanosecond remainder fits u32"),
    ))
}

fn parse_corpus() -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in CORPUS.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        if item.schema != 1 || item.mode != "plan" || item.id.is_empty() {
            return Err(format!("{} has an invalid identity or mode", item.id));
        }
        if !ids.insert(item.id.clone()) {
            return Err(format!("duplicate case id {}", item.id));
        }
        if parse_duration(&item.id, "maximum_ns", &item.maximum_ns)? != MAX_TIMEOUT {
            return Err(format!("{} has the wrong maximum", item.id));
        }
        match (item.mutant_mode.as_str(), item.fixed_mutant_ns.as_deref()) {
            ("auto", None) | ("fixed", Some(_)) => {}
            _ => return Err(format!("{} has an inconsistent mutant mode", item.id)),
        }
        if item.expected_accepted != item.expected_invalid_field.is_none() {
            return Err(format!("{} has inconsistent expected acceptance", item.id));
        }
        if let Some(field) = item.expected_invalid_field.as_deref()
            && !matches!(
                field,
                "analyzer_timeout" | "baseline_timeout" | "mutant_timeout" | "total_timeout"
            )
        {
            return Err(format!("{} has an unknown invalid field {field}", item.id));
        }
        cases.push(item);
    }
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn base_raw() -> RawRunConfig {
    RawRunConfig {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    }
}

fn raw_for(item: &OracleCase) -> RawRunConfig {
    let mut raw = base_raw();
    raw.limits.baseline_timeout =
        parse_duration(&item.id, "baseline_ns", &item.baseline_ns).unwrap();
    raw.limits.total_timeout = parse_duration(&item.id, "total_ns", &item.total_ns).unwrap();
    raw.limits.mutant_timeout = item
        .fixed_mutant_ns
        .as_deref()
        .map(|value| parse_duration(&item.id, "fixed_mutant_ns", value).unwrap());
    raw
}

fn duration_json(duration: Duration) -> serde_json::Value {
    serde_json::json!({
        "secs": duration.as_secs(),
        "nanos": duration.subsec_nanos()
    })
}

fn normalized_plan_for(item: &OracleCase) -> Result<PlanConfig, serde_json::Error> {
    let mut value = serde_json::to_value(
        RunConfig::try_from(base_raw())
            .unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap()
            .into_plan_config(
                hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
            ),
    )
    .unwrap();
    let raw = raw_for(item);
    value["discovery"]["analyzer_timeout"] =
        duration_json(parse_duration(&item.id, "analyzer_ns", &item.analyzer_ns).unwrap());
    value["limits"]["baseline_timeout"] = duration_json(raw.limits.baseline_timeout);
    value["limits"]["total_timeout"] = duration_json(raw.limits.total_timeout);
    value["limits"]["mutant_timeout"] = match raw.limits.mutant_timeout {
        None => serde_json::json!("auto"),
        Some(duration) => serde_json::json!({"fixed": duration_json(duration)}),
    };
    serde_json::from_value(value)
}

fn expected_result(item: &OracleCase) -> Result<(), ConfigError> {
    item.expected_invalid_field
        .as_deref()
        .map_or(Ok(()), |field| {
            let field = match field {
                "analyzer_timeout" => "analyzer_timeout",
                "baseline_timeout" => "baseline_timeout",
                "mutant_timeout" => "mutant_timeout",
                "total_timeout" => "total_timeout",
                _ => unreachable!("validated field"),
            };
            Err(ConfigError::InvalidLimit(field))
        })
}

#[test]
fn lean_timeout_limit_corpus_is_closed_and_valid() {
    let cases = parse_corpus().expect("Lean corpus must satisfy the adapter schema");
    assert_eq!(cases.len(), 15);
    assert_eq!(cases.iter().filter(|item| item.mode == "plan").count(), 15);
}

#[test]
fn raw_plan_and_saved_plan_validation_match_every_strict_lean_case() {
    for item in parse_corpus().expect("valid Lean corpus") {
        let raw = raw_for(&item);
        // Raw plan validation composes discovery validation with shared run normalization.
        let analyzer = parse_duration(&item.id, "analyzer_ns", &item.analyzer_ns).unwrap();
        let raw_result = hoimin_core::DiscoveryConfig::new(analyzer)
            .and_then(|_| RunConfig::try_from(raw.clone()).map(|_| ()));
        assert_eq!(raw_result, expected_result(&item), "raw case={}", item.id);

        let has_zero = [
            analyzer,
            raw.limits.baseline_timeout,
            raw.limits.total_timeout,
        ]
        .into_iter()
        .chain(raw.limits.mutant_timeout)
        .any(|duration| duration.is_zero());
        match normalized_plan_for(&item) {
            Err(error) => {
                assert!(
                    has_zero && !item.expected_accepted,
                    "case={} error={error}",
                    item.id
                );
                assert!(error.to_string().contains("duration"), "{error}");
            }
            Ok(plan) => {
                assert!(!has_zero, "zero must fail at serde: {}", item.id);
                assert_eq!(
                    plan.validate(),
                    expected_result(&item),
                    "normalized case={}",
                    item.id
                );
            }
        }

        let observed_effective = raw
            .limits
            .mutant_timeout
            .unwrap_or_else(|| auto_mutant_timeout(raw.limits.baseline_timeout));
        let expected_effective = parse_duration(
            &item.id,
            "expected_effective_mutant_ns",
            &item.expected_effective_mutant_ns,
        )
        .unwrap();
        assert_eq!(
            observed_effective, expected_effective,
            "effective case={}",
            item.id
        );
    }
}

#[test]
fn accepted_auto_timeout_cases_stay_within_the_maximum() {
    for item in parse_corpus()
        .expect("valid Lean corpus")
        .into_iter()
        .filter(|item| item.expected_accepted && item.mutant_mode == "auto")
    {
        let config = RunConfig::try_from(raw_for(&item))
            .unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let MutantTimeout::Auto = config.limits.mutant_timeout else {
            panic!("expected auto timeout: {}", item.id)
        };

        assert!(
            auto_mutant_timeout(config.limits.baseline_timeout.get()) <= MAX_TIMEOUT,
            "case={}",
            item.id
        );
    }
}

#[test]
fn accepted_fixed_timeout_cases_stay_within_the_maximum() {
    for item in parse_corpus()
        .expect("valid Lean corpus")
        .into_iter()
        .filter(|item| item.expected_accepted && item.mutant_mode == "fixed")
    {
        let config = RunConfig::try_from(raw_for(&item))
            .unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let MutantTimeout::Fixed(duration) = config.limits.mutant_timeout else {
            panic!("expected fixed timeout: {}", item.id)
        };

        assert!(duration.get() <= MAX_TIMEOUT, "case={}", item.id);
    }
}
