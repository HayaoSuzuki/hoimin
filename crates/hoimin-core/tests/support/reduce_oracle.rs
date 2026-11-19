use hoimin_core::reduce::{self, Context, Event, Proposal, ResultEntry, Span, State, Triple};
use serde::Deserialize;
use serde_json::{Value, json};

fn decode<T: serde::de::DeserializeOwned>(value: &Value) -> T {
    serde_json::from_str(&value.to_string())
        .unwrap_or_else(|error| panic!("infrastructure error constructing oracle fixture: {error}"))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    numerator: u128,
    denominator: u128,
    original: u128,
    reduced: u128,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Results {
    selected: Vec<u64>,
    before: Vec<ResultEntry>,
    after: Vec<ResultEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Trace {
    context: Context,
    deadline: u128,
    events: Vec<Event>,
}

fn snapshot(state: &State, context: &Context) -> Value {
    json!({"now":state.now,"stopped":state.stopped,"best":state.best.as_ref().map(|p| p.id),
        "safe":state.best.as_ref().is_none_or(|p| context.check(p).certified && p.completed_at < state.deadline)})
}

pub fn observe(row: &Value) -> Value {
    assert_eq!(row["schema"], 1);
    assert_eq!(row["contract"], "agreed");
    let input = &row["input"];
    match row["kind"].as_str().unwrap() {
        "certification" => {
            let context: Context = decode(&input["context"]);
            let proposal: Proposal = decode(&input["proposal"]);
            serde_json::to_value(context.check(&proposal)).unwrap()
        }
        "pair" => {
            let p: Pair = decode(input);
            json!(reduce::pair_pass(
                p.numerator,
                p.denominator,
                p.original,
                p.reduced
            ))
        }
        "results" => {
            let r: Results = decode(input);
            json!(reduce::exact_results(&r.selected, &r.before, &r.after))
        }
        "median" => {
            let t: Triple = decode(input);
            json!(reduce::median(t.values()))
        }
        "overlap" => {
            let removed: Span = decode(&input["removed"]);
            let kept: Span = decode(&input["kept"]);
            json!(removed.overlaps(&kept))
        }
        "trace" => {
            let t: Trace = decode(input);
            let mut state = State::new(t.deadline);
            let mut states = vec![snapshot(&state, &t.context)];
            for event in t.events {
                state.step(&t.context, event);
                states.push(snapshot(&state, &t.context));
            }
            json!(states)
        }
        unknown => panic!("unknown oracle kind: {unknown}"),
    }
}
