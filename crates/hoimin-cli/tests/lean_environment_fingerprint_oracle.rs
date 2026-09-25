#[path = "support/environment.rs"]
mod environment;

use std::collections::BTreeSet;
use std::ffi::OsStr;

use environment::{FLAG, Fixture, Run};
use serde::Deserialize;
use serde_json::{Value, json};

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/environment-fingerprint.jsonl");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    id: String,
    mode: String,
    tracked: bool,
    #[serde(deserialize_with = "required_option")]
    before: Option<String>,
    #[serde(deserialize_with = "required_option")]
    after: Option<String>,
    reuse: bool,
    fresh_status: String,
    initial: Expected,
    resumed: Expected,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    status: String,
    #[serde(deserialize_with = "required_option")]
    termination_exit: Option<i32>,
    executed: usize,
    exit: i32,
    complete: bool,
    baseline_exit: i32,
    other_status: String,
}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn cases(source: &str) -> Result<Vec<Case>, String> {
    let rows: Vec<Case> = source
        .lines()
        .map(|line| serde_json::from_str(line).map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    let mut ids = BTreeSet::new();
    let mut tuples = BTreeSet::new();
    for row in &rows {
        if row.schema != 1
            || row.id.is_empty()
            || row.mode != "strict"
            || !ids.insert(&row.id)
            || !tuples.insert((row.tracked, &row.before, &row.after))
        {
            return Err("invalid schema, mode, id, or duplicate input tuple".into());
        }
        for value in [&row.before, &row.after] {
            if !matches!(value.as_deref(), None | Some("" | "1" | "0")) {
                return Err("unknown environment domain value".into());
            }
        }
        for expected in [&row.initial, &row.resumed] {
            if !matches!(expected.status.as_str(), "killed" | "survived")
                || !matches!(expected.termination_exit, None | Some(0 | 1))
                || expected.executed > 1
                || !matches!(expected.exit, 0..=4)
                || expected.other_status != "not_run"
            {
                return Err("invalid observation domain".into());
            }
        }
        if !matches!(row.fresh_status.as_str(), "killed" | "survived") {
            return Err("invalid fresh verdict".into());
        }
    }
    if rows.len() != 32 {
        return Err("incomplete environment case product".into());
    }
    Ok(rows)
}

fn assert_observation(id: &str, run: &Run, expected: &Expected) {
    let id = format!("semantic mismatch case={id}");
    assert_eq!(run.exit_code, expected.exit, "{id} exit");
    assert_eq!(
        run.report["baseline"]["termination"]["Exit"], expected.baseline_exit,
        "{id} baseline"
    );
    assert_eq!(
        run.report["summary"]["complete"], expected.complete,
        "{id} complete"
    );
    assert_eq!(
        run.report["mutants"][0]["status"], expected.status,
        "{id} status"
    );
    assert_eq!(
        run.report["mutants"][0]["termination"],
        expected
            .termination_exit
            .map_or(Value::Null, |code| json!({"Exit": code})),
        "{id} termination"
    );
    assert_eq!(
        run.report["mutants"][1]["status"], expected.other_status,
        "{id} other"
    );
    assert_eq!(run.metrics["executed"], expected.executed, "{id} executed");
}

#[test]
fn corpus_is_closed_unique_and_complete() {
    assert_eq!(cases(CORPUS).unwrap().len(), 32);
    let mut rows = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    for mutation in 0..5 {
        let mut changed = rows.clone();
        match mutation {
            0 => {
                changed[0]["extra"] = json!(true);
            }
            1 => {
                changed[0]["schema"] = json!(2);
            }
            2 => {
                changed[0] = changed[1].clone();
            }
            3 => {
                changed[0]["before"] = json!("other");
            }
            _ => {
                changed[0].as_object_mut().unwrap().remove("before");
            }
        }
        let encoded = changed
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(cases(&encoded).is_err());
    }
    rows.pop();
    assert!(
        cases(
            &rows
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
        .is_err()
    );
}

#[tokio::test]
async fn public_environment_resume_matches_all_lean_cases() {
    for row in cases(CORPUS).unwrap() {
        let fixture = Fixture::new();
        let names = if row.tracked { vec![FLAG] } else { vec![] };
        let initial = fixture
            .run(
                "session.sqlite",
                false,
                &names,
                row.before.as_deref().map(OsStr::new),
            )
            .await;
        let resumed = fixture
            .run(
                "session.sqlite",
                true,
                &names,
                row.after.as_deref().map(OsStr::new),
            )
            .await;
        assert_observation(&row.id, &initial, &row.initial);
        assert_observation(&row.id, &resumed, &row.resumed);
        assert_eq!(
            initial.report["run"]["run_id"] == resumed.report["run"]["run_id"],
            row.reuse,
            "{} reuse",
            row.id
        );
        assert_eq!(
            initial.report["mutants"][0]["candidate"]["id"],
            resumed.report["mutants"][0]["candidate"]["id"],
            "{} candidate",
            row.id
        );
        if row.tracked {
            assert_eq!(
                resumed.report["mutants"][0]["status"], row.fresh_status,
                "{} fresh equivalence",
                row.id
            );
        }
    }
}
