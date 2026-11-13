//! Test-only projection so report content assertions can address named sections.
#[allow(dead_code)]
pub fn from_slice<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    if bytes.is_empty() {
        return serde_json::from_slice(bytes);
    }
    // Mixed plan/run harnesses also return single JSON plan or preview documents.
    // Reject the retired aggregate report shape so assertions still catch regressions.
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes)
        && matches!(
            value["kind"].as_str(),
            None | Some("plan" | "verify_preview")
        )
        && value.get("run").is_none()
    {
        return serde_json::from_value(value);
    }
    let mut report =
        serde_json::json!({"run": null, "baseline": null, "mutants": [], "summary": null});
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let event: serde_json::Value = serde_json::from_slice(line)?;
        match event["kind"].as_str() {
            Some("run_started") => {
                report["schema_version"] = event["schema_version"].clone();
                report["run"] = event;
            }
            Some("baseline_finished") => report["baseline"] = event,
            Some("mutant_finished") => report["mutants"].as_array_mut().unwrap().push(event),
            Some("run_finished") => report["summary"] = event,
            Some("mutant_started") => {}
            _ => panic!("unexpected report event: {event}"),
        }
    }
    serde_json::from_value(report)
}

#[allow(dead_code)]
pub fn to_vec(document: &serde_json::Value) -> Vec<u8> {
    let mut events = vec![document["run"].clone()];
    if !document["baseline"].is_null() {
        events.push(document["baseline"].clone());
    }
    for mutant in document["mutants"].as_array().unwrap() {
        events.push(serde_json::json!({
            "kind": "mutant_started", "schema_version": mutant["schema_version"],
            "run_id": mutant["run_id"], "mutant_id": mutant["candidate"]["id"],
            "mutant_sequence": mutant["candidate"]["sequence"]
        }));
        events.push(mutant.clone());
    }
    events.push(document["summary"].clone());
    for (index, event) in events.iter_mut().enumerate() {
        event["sequence"] = (index + 1).into();
    }
    events
        .iter()
        .map(|event| event.to_string() + "\n")
        .collect::<String>()
        .into_bytes()
}

#[cfg(test)]
mod tests {
    #[rstest::rstest]
    #[case::missing_kind(b"{\"schema_version\":1,\"run\":null}")]
    #[case::retired_aggregate(b"{\"kind\":\"retired_report\",\"run\":null}")]
    #[case::unknown_kind(b"{\"kind\":\"unknown_event\"}")]
    #[should_panic(expected = "unexpected report event")]
    fn rejects_events_outside_the_jsonl_protocol(#[case] bytes: &[u8]) {
        let _: serde_json::Value = super::from_slice(bytes).unwrap();
    }
}

#[cfg(test)]
#[rstest::rstest]
#[case::empty(b"")]
#[case::malformed(b"{")]
fn malformed_input_is_a_json_error(#[case] input: &[u8]) {
    assert!(from_slice::<serde_json::Value>(input).is_err());
}

#[cfg(test)]
#[rstest::rstest]
#[case::without_baseline(&serde_json::Value::Null, 3, 4)]
#[case::with_baseline(&serde_json::json!({"kind": "baseline_finished"}), 4, 5)]
fn serialized_lifecycle_roundtrips_with_ordered_sequences(
    #[case] baseline: &serde_json::Value,
    #[case] mutant_sequence: u64,
    #[case] summary_sequence: u64,
) {
    let document = serde_json::json!({
        "run": {"kind": "run_started", "schema_version": 6, "run_id": "r"},
        "baseline": baseline,
        "mutants": [{"kind": "mutant_finished", "schema_version": 6, "run_id": "r", "candidate": {"id": "m", "sequence": 1}}],
        "summary": {"kind": "run_finished", "run_id": "r"},
    });
    let decoded: serde_json::Value = from_slice(&to_vec(&document)).unwrap();
    assert_eq!(decoded["run"]["sequence"], 1);
    assert_eq!(decoded["run"]["run_id"], "r");
    assert_eq!(
        decoded["baseline"].is_null(),
        document["baseline"].is_null()
    );
    assert_eq!(decoded["mutants"][0]["sequence"], mutant_sequence);
    assert_eq!(decoded["summary"]["sequence"], summary_sequence);
}

#[cfg(test)]
#[rstest::rstest]
#[case::plain(&serde_json::json!({"answer": 1}))]
#[case::plan(&serde_json::json!({"kind": "plan"}))]
#[case::preview(&serde_json::json!({"kind": "verify_preview"}))]
fn standalone_documents_retain_their_original_shape(#[case] document: &serde_json::Value) {
    let decoded: serde_json::Value = from_slice(&serde_json::to_vec(document).unwrap()).unwrap();
    assert_eq!(&decoded, document);
}
