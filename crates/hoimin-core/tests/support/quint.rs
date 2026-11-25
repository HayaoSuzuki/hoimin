//! ITF encoding stays here; target adapters only interpret their own variables.
use serde_json::Value;

pub fn number(value: &Value) -> u64 {
    value["#bigint"]
        .as_str()
        .expect("ITF integer")
        .parse()
        .expect("nonnegative u64")
}

pub fn set(value: &Value) -> &Vec<Value> {
    value["#set"].as_array().expect("ITF set")
}

pub fn states(trace: &str) -> Vec<Value> {
    let trace: Value = serde_json::from_str(trace).expect("ITF JSON");
    assert_eq!(trace["#meta"]["format"], "ITF", "ITF format");
    assert_eq!(trace["#meta"]["status"], "ok", "successful model trace");
    trace["states"]
        .as_array()
        .expect("ITF states")
        .iter()
        .map(|state| {
            let mut fields = serde_json::Map::new();
            for (qualified, value) in state.as_object().expect("ITF state") {
                if qualified == "#meta" {
                    continue;
                }
                let name = qualified.rsplit("::").next().unwrap();
                assert!(
                    fields.insert(name.to_owned(), value.clone()).is_none(),
                    "ambiguous ITF variable: {name}"
                );
            }
            Value::Object(fields)
        })
        .collect()
}
