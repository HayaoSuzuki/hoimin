#[path = "progress_duplicate_cases.rs"]
mod cases;

use super::{jsonl_events, run_progress, valid_report};
use cases::{Mode, Reason, paths, read_cases};
use serde_json::{Value, json};
use std::fs;

fn encode_report(document: &Value) -> String {
    jsonl_events(document)
        .iter()
        .map(|event| event.to_string() + "\n")
        .collect()
}

fn reports() -> [String; 3] {
    let complete = valid_report();
    let compact = encode_report(&complete);
    let mut with_newline = compact.clone();
    with_newline.push('\n');
    let mut incomplete = complete;
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    [compact, with_newline, encode_report(&incomplete)]
}

#[tokio::test]
async fn progress_duplicate_warnings_and_comparisons_match_lean() {
    let documents = reports();
    let mut count = 0;
    for case in read_cases()
        .into_iter()
        .filter(|case| case.mode == Mode::Strict)
    {
        count += 1;
        let id = &case.id;
        let directory = tempfile::tempdir().unwrap();
        let paths = paths(&directory, &case);
        for (path, content) in paths.iter().zip(case.initial) {
            fs::write(path, &documents[usize::from(content.unwrap())]).unwrap();
        }
        let reports: Vec<_> = case
            .actions
            .iter()
            .map(|action| paths[action.path].clone())
            .collect();
        let expected_warnings: Vec<_> = case.expected.iter().enumerate().filter_map(|(index, duplicate)| {
            let duplicate = duplicate.as_ref()?;
            let label = match duplicate.reason {
                Reason::SamePath => "same path",
                Reason::IdenticalBytes => "identical bytes",
            };
            Some(format!(
                "warning: progress input {} ({}) repeats input {} ({}): {label}; comparisons are unchanged",
                index + 1,
                reports[index].display(),
                duplicate.previous + 1,
                reports[duplicate.previous].display(),
            ))
        }).collect();

        let (code, stdout, stderr) = run_progress(&reports).await;

        let stderr = String::from_utf8(stderr).unwrap();
        assert_eq!(code, 0, "{id}: {stderr}");
        let warnings: Vec<_> = stderr
            .lines()
            .filter(|line| line.starts_with("warning: progress input "))
            .collect();
        assert_eq!(warnings, expected_warnings, "{id}");
        let output: Value = serde_json::from_slice(&stdout).unwrap();
        let inputs = output["inputs"].as_array().unwrap();
        let comparisons: Vec<_> = output["comparisons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|comparison| &comparison["state"])
            .collect();
        assert_eq!(
            json!({
                "inputs": inputs.len(),
                "comparisons": comparisons,
                "consecutive_stalls": output["consecutive_stalls"],
                "latest": output["latest"]["state"],
            }),
            case.history.unwrap(),
            "{id}",
        );
        for (input, path) in inputs.iter().zip(&reports) {
            assert_eq!(input["source"].as_str(), path.to_str(), "{id}");
        }
    }
    assert_eq!(count, 84);
}
