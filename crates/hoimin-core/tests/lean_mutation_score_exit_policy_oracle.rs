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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
struct RunFlags {
    infrastructure_error: bool,
    baseline_failed: bool,
    incomplete: bool,
    interrupted: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
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
    let cases = parse_rows(input)?;
    validate_corpus(&cases)?;
    Ok(cases)
}

fn parse_rows(input: &str) -> Result<Vec<CorpusCase>, String> {
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

fn expected_ids() -> BTreeSet<String> {
    let mut ids = [
        "summary_empty",
        "summary_killed",
        "summary_survived",
        "summary_timeout",
        "summary_out_of_memory",
        "summary_process_limit",
        "summary_error",
        "summary_not_run",
        "summary_all_statuses",
        "summary_score_two_thirds",
        "composed_summary_error",
        "composed_run_infrastructure",
        "composed_interrupted_error_survivor",
        "composed_baseline_timeout_survivor",
        "composed_survivor_complete",
        "exact_fraction_beyond_binary64",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    for bits in 0_u8..32 {
        ids.insert(format!("policy_{bits:05b}"));
    }
    ids
}

fn validate_corpus(cases: &[CorpusCase]) -> Result<(), String> {
    let ids = cases
        .iter()
        .map(|case| case.id.clone())
        .collect::<BTreeSet<_>>();
    if ids != expected_ids() || ids.len() != cases.len() {
        return Err("corpus IDs do not match the closed case set".to_owned());
    }
    let policies = cases
        .iter()
        .filter(|case| case.scenario == "exit_policy")
        .map(|case| case.direct_policy)
        .collect::<BTreeSet<_>>();
    if policies.len() != 32 {
        return Err("exit policy rows do not cover 32 unique assignments".to_owned());
    }
    Ok(())
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
    let default_flags = RunFlags::default();
    let default_policy = Policy::default();
    match item.scenario.as_str() {
        "summary" if item.run_flags != default_flags || item.direct_policy != default_policy => {
            return Err("summary row has unrelated premises".to_owned());
        }
        "exit_policy"
            if !item.statuses.is_empty()
                || item.run_flags != default_flags
                || item.expected_score.is_some() =>
        {
            return Err("exit policy row has unrelated premises".to_owned());
        }
        "composed" if item.direct_policy != default_policy => {
            return Err("composed row has unrelated premises".to_owned());
        }
        "exact_fraction"
            if !item.statuses.is_empty()
                || item.run_flags != default_flags
                || item.direct_policy != default_policy =>
        {
            return Err("exact fraction row has unrelated premises".to_owned());
        }
        _ => {}
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

#[allow(clippy::cast_precision_loss)]
fn strict_score_projection(score: Fraction) -> f64 {
    const MAX_EXACT_BINARY64_INTEGER: u64 = 1 << 53;
    assert!(score.numerator <= MAX_EXACT_BINARY64_INTEGER);
    assert!(score.denominator <= MAX_EXACT_BINARY64_INTEGER);
    score.numerator as f64 / score.denominator as f64
}

#[test]
fn corpus_is_closed_typed_and_exhaustive() {
    let cases = parse_corpus(CORPUS).unwrap();
    assert_eq!(cases.len(), 48);
    let ids = cases
        .iter()
        .map(|case| case.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids, expected_ids());
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
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.scenario == "exit_policy")
            .map(|case| case.direct_policy)
            .collect::<BTreeSet<_>>()
            .len(),
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
        let expected_score = case.expected_score.map(strict_score_projection);
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
    assert!(parse_rows(&format!("{value}\n")).is_err());

    let mut crossed: serde_json::Value = serde_json::from_str(first).unwrap();
    crossed["mode"] = serde_json::json!("model-only");
    assert!(parse_rows(&format!("{crossed}\n")).is_err());

    let mut unknown_status: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown_status["statuses"] = serde_json::json!(["unknown"]);
    assert!(parse_rows(&format!("{unknown_status}\n")).is_err());
    assert!(status("unknown").is_err());

    let mut unrelated: serde_json::Value = serde_json::from_str(first).unwrap();
    unrelated["run_flags"]["incomplete"] = serde_json::json!(true);
    assert!(parse_rows(&format!("{unrelated}\n")).is_err());

    let mut renamed = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    renamed[0]["id"] = serde_json::json!("summary_killed");
    let renamed = renamed
        .into_iter()
        .map(|row| row.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(parse_corpus(&renamed).is_err());
}
