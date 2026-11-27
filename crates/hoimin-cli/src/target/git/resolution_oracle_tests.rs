use super::{GitError, GitRunner, ProcessCancellation, resolve_changed_scoped};
use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{LineRange, TargetError, TargetSlice};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

const REPLAY_ROOT: &str = "HOIMIN_TEST_GIT_RESOLUTION_ROOT";
const RESPONDER: &str = r#"#!/bin/sh
index=0
if [ -f request-count ]; then
    read -r index < request-count
fi
index=$((index + 1))
printf '%s\n' "$index" > request-count
printf '%s\000' "$@" > "args-$index"
if [ ! -f "status-$index" ]; then
    printf '%s\n' "unexpected extra Git request" >&2
    exit 97
fi
read -r status < "status-$index"
cat "stdout-$index"
if [ "$status" -ne 0 ]; then
    printf '%s\n' "fixture failure" >&2
fi
exit "$status"
"#;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Ok {
        changed: BTreeMap<Utf8PathBuf, Vec<LineRange>>,
    },
    Repository,
    Command,
    OidUtf8,
    OidForm,
    Inventory,
    Hunk,
}

#[derive(serde::Deserialize)]
struct Reply {
    status: u8,
    bytes: Vec<u8>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    revision: Option<String>,
    context: u32,
    scope: Option<Vec<Utf8PathBuf>>,
    responses: Vec<Reply>,
    calls: Vec<Vec<String>>,
    sources: BTreeMap<Utf8PathBuf, Vec<u8>>,
    expected: Outcome,
}

fn observe(error: GitError, id: &str) -> Outcome {
    match error {
        GitError::Failed(TargetError::GitRepositoryRequired) => Outcome::Repository,
        GitError::Failed(TargetError::GitFailed(message)) => match message.as_str() {
            "Git commit ID is not valid UTF-8" => Outcome::OidUtf8,
            "git rev-parse returned an invalid commit ID" => Outcome::OidForm,
            "invalid Git name-status inventory" => Outcome::Inventory,
            "invalid Git hunk start" => Outcome::Hunk,
            message if message.starts_with("git ") && message.ends_with(": fixture failure") => {
                Outcome::Command
            }
            _ => panic!("{id}: unexpected Git diagnostic: {message}"),
        },
        error => panic!("{id}: unexpected Git error: {error:?}"),
    }
}

async fn replay(evidence: &Path) {
    let corpus = include_str!("../../../../../formal/HoiminOracle/corpus/git-resolution.jsonl");
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        let root = tempfile::tempdir_in(evidence).unwrap();
        let root_path = Utf8Path::from_path(root.path()).unwrap();
        for (index, response) in (1..).zip(fixture.responses) {
            fs::write(
                root_path.join(format!("status-{index}")),
                format!("{}\n", response.status),
            )
            .unwrap();
            fs::write(root_path.join(format!("stdout-{index}")), response.bytes).unwrap();
        }
        for (path, contents) in fixture.sources {
            let path = root_path.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let targets = fixture.scope.map(|paths| {
            paths
                .into_iter()
                .map(|path| TargetSlice {
                    path,
                    lines: Vec::new(),
                    symbols: Vec::new(),
                })
                .collect::<Vec<_>>()
        });
        let observed = resolve_changed_scoped(
            root_path,
            fixture.revision.as_deref(),
            targets.as_deref(),
            fixture.context,
            &GitRunner::default(),
            &ProcessCancellation::new(),
        )
        .await;
        let request_count: usize = fs::read_to_string(root_path.join("request-count"))
            .unwrap_or_else(|error| panic!("{}: missing Git transcript: {error}", fixture.id))
            .trim()
            .parse()
            .unwrap();
        let calls: Vec<Vec<String>> = (1..=request_count)
            .map(|index| {
                fs::read_to_string(root_path.join(format!("args-{index}")))
                    .unwrap()
                    .strip_suffix('\0')
                    .unwrap()
                    .split('\0')
                    .map(str::to_owned)
                    .collect()
            })
            .collect();
        assert_eq!(calls, fixture.calls, "{}", fixture.id);
        let outcome = match observed {
            Ok(changed) => Outcome::Ok { changed },
            Err(error) => observe(error, &fixture.id),
        };
        assert_eq!(outcome, fixture.expected, "{}", fixture.id);
    }
    assert_eq!(ids.len(), 36);
    fs::write(evidence.join("replayed"), ids.len().to_string()).unwrap();
}

#[tokio::test]
async fn git_resolution_calls_match_lean() {
    if let Some(evidence) = std::env::var_os(REPLAY_ROOT) {
        replay(Path::new(&evidence)).await;
        return;
    }

    // Keep PATH changes local to the child test. The resolver and runner then
    // start a separate fixture Git process through their normal spawn path.
    let directory = tempfile::tempdir().unwrap();
    let bin = directory.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let git = bin.join("git");
    fs::write(&git, RESPONDER).unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(60),
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "target::git::resolution_oracle_tests::git_resolution_calls_match_lean",
                "--nocapture",
            ])
            .env(REPLAY_ROOT, directory.path())
            .env("PATH", path)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("Git protocol replay timed out")
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("replayed")).unwrap(),
        "36"
    );
}
