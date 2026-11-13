#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::time::Duration;

use camino::Utf8PathBuf;
use hoimin_budget_math::{MutantTimeout, project_top_budget};
use hoimin_core::{CommandArg, RawRunConfig, RunConfig};
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/top-budget-projection.jsonl");
const NANOS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    selected: String,
    jobs: String,
    planned_total_timeout_ns: String,
    baseline_ns: String,
    timeout_mode: String,
    fixed_timeout_ns: Option<String>,
    remaining_ns: String,
    duration_max_ns: String,
    expected_effective_timeout_ns: String,
    expected_waves: String,
    expected_capacity_ns: String,
    expected_shortfall: bool,
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        validate_case(&item)?;
        if !ids.insert(item.id.clone()) {
            return Err(format!("duplicate case id {}", item.id));
        }
        cases.push(item);
    }
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if item.schema != 1 {
        return Err(format!(
            "{} has unsupported schema {}",
            item.id, item.schema
        ));
    }
    if !matches!(item.mode.as_str(), "strict" | "model-only") || item.id.is_empty() {
        return Err(format!("{} has an invalid identity or mode", item.id));
    }
    let jobs = parse_u128(&item.id, "jobs", &item.jobs)?;
    if jobs == 0 {
        return Err(format!("{} has zero jobs", item.id));
    }
    let duration_max = parse_u128(&item.id, "duration_max_ns", &item.duration_max_ns)?;
    if duration_max != Duration::MAX.as_nanos() {
        return Err(format!("{} has the wrong Duration::MAX", item.id));
    }
    for (field, value) in [
        ("selected", &item.selected),
        ("planned_total_timeout_ns", &item.planned_total_timeout_ns),
        ("baseline_ns", &item.baseline_ns),
        ("remaining_ns", &item.remaining_ns),
        (
            "expected_effective_timeout_ns",
            &item.expected_effective_timeout_ns,
        ),
        ("expected_waves", &item.expected_waves),
        ("expected_capacity_ns", &item.expected_capacity_ns),
    ] {
        let value = parse_u128(&item.id, field, value)?;
        if field.ends_with("_ns") && value > Duration::MAX.as_nanos() {
            return Err(format!("{} has {field} beyond Duration::MAX", item.id));
        }
    }
    match (item.timeout_mode.as_str(), item.fixed_timeout_ns.as_deref()) {
        ("auto", None) => {}
        ("fixed", Some(value)) if parse_u128(&item.id, "fixed_timeout_ns", value)? > 0 => {}
        _ => return Err(format!("{} has an inconsistent timeout mode", item.id)),
    }
    Ok(())
}

fn parse_u128(id: &str, field: &str, value: &str) -> Result<u128, String> {
    value
        .parse()
        .map_err(|error| format!("{id} has invalid {field}: {error}"))
}

fn duration_from_nanos(value: u128) -> Result<Duration, String> {
    if value > Duration::MAX.as_nanos() {
        return Err(format!("duration {value} exceeds Duration::MAX"));
    }
    // The maximum-duration check above bounds seconds by u64::MAX.
    let seconds =
        u64::try_from(value / NANOS_PER_SECOND).expect("bounded duration seconds fit u64");
    // A nanosecond remainder is strictly below one billion.
    let nanos = u32::try_from(value % NANOS_PER_SECOND).expect("nanosecond remainder fits u32");
    Ok(Duration::new(seconds, nanos))
}

fn duration_field(item: &OracleCase, field: &str, value: &str) -> Result<Duration, String> {
    duration_from_nanos(parse_u128(&item.id, field, value)?)
}

fn fixed_timeout(value: Duration) -> Result<MutantTimeout, String> {
    let mut raw = RawRunConfig {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    };
    raw.limits.mutant_timeout = Some(value);
    RunConfig::try_from(raw)
        .map(|config| config.limits.mutant_timeout)
        .map_err(|error| format!("cannot construct fixed timeout: {error}"))
}

fn timeout(item: &OracleCase) -> Result<MutantTimeout, String> {
    match (item.timeout_mode.as_str(), item.fixed_timeout_ns.as_deref()) {
        ("auto", None) => Ok(MutantTimeout::Auto),
        ("fixed", Some(value)) => fixed_timeout(duration_field(item, "fixed_timeout_ns", value)?),
        _ => Err(format!("{} has an inconsistent timeout mode", item.id)),
    }
}

#[test]
fn lean_top_budget_projection_corpus_is_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean corpus must satisfy the adapter schema");
    assert_eq!(cases.len(), 8);
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 8);
}

#[test]
fn public_projection_matches_every_strict_lean_case() {
    let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
    let mut compared = 0;
    for item in cases.iter().filter(|item| item.mode == "strict") {
        let selected_u128 = parse_u128(&item.id, "selected", &item.selected).unwrap();
        // All corpus counts fit u64; only 32-bit hosts need to skip an unrepresentable case.
        #[cfg(not(target_pointer_width = "32"))]
        let selected = usize::try_from(selected_u128).expect("corpus counts fit a 64-bit usize");
        #[cfg(target_pointer_width = "32")]
        let Ok(selected) = usize::try_from(selected_u128) else {
            eprintln!(
                "infrastructure-error: {} selected={} is not configurable on this platform",
                item.id, item.selected
            );
            continue;
        };
        let jobs = usize::try_from(parse_u128(&item.id, "jobs", &item.jobs).unwrap())
            .ok()
            .and_then(NonZeroUsize::new)
            .expect("validated representable positive jobs");
        let planned_total_timeout = duration_field(
            item,
            "planned_total_timeout_ns",
            &item.planned_total_timeout_ns,
        )
        .unwrap();
        let baseline = duration_field(item, "baseline_ns", &item.baseline_ns).unwrap();
        let remaining = duration_field(item, "remaining_ns", &item.remaining_ns).unwrap();
        let projection = project_top_budget(
            selected,
            jobs,
            planned_total_timeout,
            baseline,
            timeout(item).unwrap(),
            remaining,
        );

        assert_eq!(projection.selected, selected, "case={}", item.id);
        assert_eq!(projection.jobs, jobs.get(), "case={}", item.id);
        assert_eq!(
            projection.planned_total_timeout, planned_total_timeout,
            "case={}",
            item.id
        );
        assert_eq!(projection.baseline, baseline, "case={}", item.id);
        assert_eq!(projection.remaining, remaining, "case={}", item.id);
        assert_eq!(
            projection.effective_mutant_timeout,
            duration_field(
                item,
                "expected_effective_timeout_ns",
                &item.expected_effective_timeout_ns,
            )
            .unwrap(),
            "case={}",
            item.id
        );
        assert_eq!(
            projection.waves,
            usize::try_from(parse_u128(&item.id, "expected_waves", &item.expected_waves).unwrap())
                .expect("expected waves fit when selected fits"),
            "case={}",
            item.id
        );
        assert_eq!(
            projection.projected_capacity,
            duration_field(item, "expected_capacity_ns", &item.expected_capacity_ns).unwrap(),
            "case={}",
            item.id
        );
        assert_eq!(
            projection.is_shortfall(),
            item.expected_shortfall,
            "case={}",
            item.id
        );
        compared += 1;
    }
    assert!(compared >= 7, "too few strict cases were comparable");
}

#[rstest::rstest]
#[case(|item: &mut OracleCase| item.schema = 2, "unsupported schema")]
#[case(|item: &mut OracleCase| item.id.clear(), "invalid identity or mode")]
#[case(|item: &mut OracleCase| item.mode = "unknown".into(), "invalid identity or mode")]
#[case(|item: &mut OracleCase| item.jobs = "0".into(), "zero jobs")]
#[case(|item: &mut OracleCase| item.jobs = "invalid".into(), "invalid jobs")]
#[case(|item: &mut OracleCase| item.duration_max_ns = "0".into(), "wrong Duration::MAX")]
#[case(|item: &mut OracleCase| item.duration_max_ns = "invalid".into(), "invalid duration_max_ns")]
#[case(|item: &mut OracleCase| item.selected = "invalid".into(), "invalid selected")]
#[case(|item: &mut OracleCase| item.baseline_ns = u128::MAX.to_string(), "beyond Duration::MAX")]
#[case(|item: &mut OracleCase| { item.timeout_mode = "fixed".into(); item.fixed_timeout_ns = Some("0".into()); }, "inconsistent timeout mode")]
#[case(|item: &mut OracleCase| { item.timeout_mode = "fixed".into(); item.fixed_timeout_ns = Some("invalid".into()); }, "invalid fixed_timeout_ns")]
fn projection_adapter_rejects_invalid_case_metadata(
    #[case] mutate: fn(&mut OracleCase),
    #[case] expected: &str,
) {
    let mut item = parse_corpus(CORPUS).unwrap().remove(0);
    mutate(&mut item);
    assert!(validate_case(&item).unwrap_err().contains(expected));
}

#[test]
fn projection_adapter_rejects_empty_input() {
    assert_eq!(
        parse_corpus("\n\n").unwrap_err(),
        "corpus contains no cases"
    );
}

#[test]
fn projection_adapter_rejects_invalid_json() {
    assert!(
        parse_corpus("{")
            .unwrap_err()
            .contains("line 1 is invalid JSON")
    );
}

#[test]
fn projection_adapter_rejects_duplicate_ids() {
    let line = CORPUS.lines().next().unwrap();
    assert!(
        parse_corpus(&format!("{line}\n{line}"))
            .unwrap_err()
            .contains("duplicate case id")
    );
}

#[test]
fn projection_adapter_rejects_duration_above_maximum() {
    assert!(
        duration_from_nanos(Duration::MAX.as_nanos() + 1)
            .unwrap_err()
            .contains("exceeds Duration::MAX")
    );
}

#[test]
fn projection_adapter_rejects_zero_fixed_timeout() {
    assert!(
        fixed_timeout(Duration::ZERO)
            .unwrap_err()
            .contains("cannot construct fixed timeout")
    );
}

#[test]
fn projection_execution_rejects_inconsistent_timeout_mode() {
    let mut item = parse_corpus(CORPUS).unwrap().remove(0);
    item.timeout_mode = "unknown".into();
    assert!(
        timeout(&item)
            .unwrap_err()
            .contains("inconsistent timeout mode")
    );
}

#[test]
fn projection_corpus_propagates_invalid_case_metadata() {
    let invalid = CORPUS.replacen("\"schema\":1", "\"schema\":2", 1);
    assert!(
        parse_corpus(&invalid)
            .unwrap_err()
            .contains("unsupported schema")
    );
}

#[test]
fn projection_duration_field_reports_invalid_numbers() {
    let item = parse_corpus(CORPUS).unwrap().remove(0);
    assert!(
        duration_field(&item, "remaining_ns", "invalid")
            .unwrap_err()
            .contains("invalid remaining_ns")
    );
}

#[test]
fn projection_timeout_reports_invalid_fixed_timeout_text() {
    let mut item = parse_corpus(CORPUS).unwrap().remove(0);
    item.timeout_mode = "fixed".into();
    item.fixed_timeout_ns = Some("invalid".into());
    assert!(
        timeout(&item)
            .unwrap_err()
            .contains("invalid fixed_timeout_ns")
    );
}
