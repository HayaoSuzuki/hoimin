use std::collections::BTreeSet;

use hoimin_core::{ExitPolicy, MutationStatus, exit_code_for, summarize};
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/mutation-score-exit-policy.jsonl");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    statuses: Vec<String>,
    run_flags: RunFlags,
    direct_policy: Policy,
    expected_counts: Counts,
    expected_score: Option<Fraction>,
    expected_policy: Policy,
    expected_complete: bool,
    expected_exit_code: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RunFlags {
    infrastructure_error: bool,
    baseline_failed: bool,
    incomplete: bool,
    interrupted: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Policy {
    infrastructure_error: bool,
    baseline_failed: bool,
    incomplete: bool,
    survivors: bool,
    interrupted: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Counts {
    killed: u64,
    survived: u64,
    timeout: u64,
    out_of_memory: u64,
    process_limit: u64,
    error: u64,
    not_run: u64,
    inconclusive: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Fraction {
    numerator: u64,
    denominator: u64,
}

fn parse_corpus(input: &str) -> Result<Vec<CorpusCase>, String> {
    input
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let item: CorpusCase = serde_json::from_str(line)
                .map_err(|error| format!("line {}: {error}", index + 1))?;
            validate_case(&item).map_err(|error| format!("line {}: {error}", index + 1))?;
            Ok(item)
        })
        .collect()
}

fn validate_case(item: &CorpusCase) -> Result<(), String> {
    if item.schema != 1 {
        return Err("unsupported schema".to_owned());
    }
    let valid_mode = matches!(
        (item.mode.as_str(), item.scenario.as_str()),
        ("strict", "summary" | "exit_policy")
            | ("internal-fixture", "composed")
            | ("model-only", "exact_fraction")
    );
    if !valid_mode {
        return Err("mode/scenario mismatch".to_owned());
    }
    for name in &item.statuses {
        status(name)?;
    }
    if item
        .expected_score
        .is_some_and(|score| score.denominator == 0)
    {
        return Err("zero score denominator".to_owned());
    }
    if item.scenario == "exit_policy"
        && (!item.statuses.is_empty()
            || item.run_flags.infrastructure_error
            || item.run_flags.baseline_failed
            || item.run_flags.incomplete
            || item.run_flags.interrupted)
    {
        return Err("exit policy row has unrelated premises".to_owned());
    }
    Ok(())
}

fn status(name: &str) -> Result<MutationStatus, String> {
    match name {
        "killed" => Ok(MutationStatus::Killed),
        "survived" => Ok(MutationStatus::Survived),
        "timeout" => Ok(MutationStatus::Timeout),
        "out_of_memory" => Ok(MutationStatus::OutOfMemory),
        "process_limit" => Ok(MutationStatus::ProcessLimit),
        "error" => Ok(MutationStatus::Error),
        "not_run" => Ok(MutationStatus::NotRun),
        _ => Err(format!("unknown status {name}")),
    }
}

impl From<Policy> for ExitPolicy {
    fn from(value: Policy) -> Self {
        Self {
            infrastructure_error: value.infrastructure_error,
            baseline_failed: value.baseline_failed,
            incomplete: value.incomplete,
            survivors: value.survivors,
            interrupted: value.interrupted,
        }
    }
}

fn policy_from_rust(value: ExitPolicy) -> Policy {
    Policy {
        infrastructure_error: value.infrastructure_error,
        baseline_failed: value.baseline_failed,
        incomplete: value.incomplete,
        survivors: value.survivors,
        interrupted: value.interrupted,
    }
}

#[test]
fn corpus_is_closed_typed_and_exhaustive() {
    let cases = parse_corpus(CORPUS).unwrap();
    assert_eq!(cases.len(), 48);
    let ids = cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), cases.len());
    assert!(cases.iter().all(|case| case.schema == 1));
    assert!(cases.iter().all(|case| matches!(
        case.mode.as_str(),
        "strict" | "internal-fixture" | "model-only"
    )));
    assert!(cases.iter().all(|case| matches!(
        case.scenario.as_str(),
        "summary" | "exit_policy" | "composed" | "exact_fraction"
    )));
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.scenario == "exit_policy")
            .count(),
        32
    );
}

#[test]
fn strict_summary_rows_match_complete_rust_observations() {
    for case in parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .filter(|case| case.mode == "strict" && case.scenario == "summary")
    {
        let statuses = case
            .statuses
            .iter()
            .map(|name| status(name))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let actual = summarize(&statuses);
        assert_eq!(
            Counts {
                killed: actual.killed,
                survived: actual.survived,
                timeout: actual.timeout,
                out_of_memory: actual.out_of_memory,
                process_limit: actual.process_limit,
                error: actual.error,
                not_run: actual.not_run,
                inconclusive: actual.inconclusive,
            },
            case.expected_counts,
            "{}",
            case.id
        );
        let expected_score = case
            .expected_score
            .map(|score| score.numerator as f64 / score.denominator as f64);
        assert_eq!(
            actual.score.map(f64::to_bits),
            expected_score.map(f64::to_bits),
            "{}",
            case.id
        );
        let policy = ExitPolicy::from_summary(&actual);
        assert_eq!(
            policy_from_rust(policy),
            case.expected_policy,
            "{}",
            case.id
        );
        assert_eq!(
            exit_code_for(policy),
            case.expected_exit_code,
            "{}",
            case.id
        );
        assert_eq!(
            !policy.infrastructure_error
                && !policy.baseline_failed
                && !policy.incomplete
                && !policy.interrupted,
            case.expected_complete,
            "{}",
            case.id
        );
    }
}

#[test]
fn all_direct_exit_policy_rows_match_rust_precedence() {
    for case in parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .filter(|case| case.scenario == "exit_policy")
    {
        assert_eq!(
            exit_code_for(case.direct_policy.into()),
            case.expected_exit_code,
            "{}",
            case.id
        );
    }
}

#[test]
fn corpus_rejects_unknown_fields_modes_and_statuses() {
    let first = CORPUS.lines().next().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(first).unwrap();
    value["unexpected"] = serde_json::json!(true);
    assert!(parse_corpus(&format!("{}\n", value)).is_err());

    let mut crossed: serde_json::Value = serde_json::from_str(first).unwrap();
    crossed["mode"] = serde_json::json!("model-only");
    assert!(parse_corpus(&format!("{}\n", crossed)).is_err());

    let mut unknown_status: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown_status["statuses"] = serde_json::json!(["unknown"]);
    assert!(parse_corpus(&format!("{}\n", unknown_status)).is_err());
    assert!(status("unknown").is_err());
}
