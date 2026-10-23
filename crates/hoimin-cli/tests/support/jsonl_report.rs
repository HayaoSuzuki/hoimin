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
