# Resource wait isolation verification (#342)

## Changes

Linux root cleanup and run close hold per-root cleanup gates across filesystem operations while releasing the shared registry lock. Counter refresh skips roots between physical deletion and registration removal. Failed cleanup restores accounting eligibility, and close retains all captured root gates through recursive run-directory removal.

Windows classification retains an `Arc<OwnedHandle>` during a process exit wait outside the registry lock. It drains queued notifications again before choosing the signaled-process fallback. A delayed notification therefore keeps its generation/PID barrier. CI retains the existing Linux runners; no Windows job is configured. Platform-specific Clippy allowances preserve the common fallible path API and Unix-only reaping transition.

The process lifecycle moves its unique supervisor into blocking classification, termination and quiescence operations. Completed task results retain the same drop-routing owner, including when the awaiting future never consumes the result. Root reaping disarms Unix process-group signaling before the first new await; probe-only checks retain the numeric ID. The normal lifecycle awaits supervisor destruction before acknowledging cleanup. Startup admission guards and the existing Linux launcher-attachment transaction remain unchanged.

## Reproduction and review

- Linux RED: root deletion and close callbacks both found the shared registry mutex locked. The root test deletes fake counters before probing the sibling operation.
- Dispatcher RED: a synchronous resource operation prevented a current-thread Tokio task from releasing it, hitting the external two-second test guard.
- Review RED: dropping a completed, unconsumed blocking result destroyed its owner on the dispatcher (`ThreadId(2) == ThreadId(2)`). The corrected result carries a drop-routing wrapper.
- Review RED: the reap transition left `Some(41)` as a signaling target. The fixture removes this synthetic target before assertions, so it cannot signal an unrelated real process. The corrected transition retains the ID only for non-signaling probes.
- Independent review and scoped rereview found no remaining Important or Critical code issues after these fixes. Windows runtime validation remains an unverified boundary.

## Local results

From the issue worktree:

```sh
CARGO_TARGET_DIR=/private/tmp/hoimin-342-target cargo test --workspace --all-features --quiet -- --test-threads=1
CARGO_TARGET_DIR=/private/tmp/hoimin-342-target cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

All passed on macOS arm64. The CLI library had 554 passed / 9 ignored; `run_e2e` had 55 passed. The ignored library cases are existing subprocess fixtures. The five owned-blocking tests cover dispatcher progress, in-flight and completed-result cancellation, destruction thread, and panic/effect identity.

Linux Docker (`rust:1.98-bookworm`, arm64):

```sh
docker exec hoimin-342-linux-tests cargo clippy --workspace --all-targets --all-features -- -D warnings
docker exec hoimin-342-linux-tests cargo test -p hoimin-cli --lib --all-features resource:: -- --test-threads=1
```

Both passed: 44 resource tests passed / 1 subprocess fixture ignored, including 36 Linux-specific tests. The seven new Linux tests exercise the production synchronization algorithm with fake cgroup files and a cleanup callback. They do not establish delegated-kernel-cgroup integration; see `2026-09-08-issue-342-linux.md`.

Windows Rust cross-check on macOS:

```sh
LIBSQLITE3_SYS_USE_PKG_CONFIG=1 PKG_CONFIG_ALLOW_CROSS=1 CARGO_TARGET_DIR=/private/tmp/hoimin-342-windows-target cargo check -p hoimin-cli --all-targets --all-features --features blake3/pure --target x86_64-pc-windows-msvc --quiet
LIBSQLITE3_SYS_USE_PKG_CONFIG=1 PKG_CONFIG_ALLOW_CROSS=1 CARGO_TARGET_DIR=/private/tmp/hoimin-342-windows-target cargo clippy -p hoimin-cli --lib --all-features --features blake3/pure --target x86_64-pc-windows-msvc -- -D warnings
```

The cross-check uses BLAKE3's pure feature and host SQLite metadata to check Windows Rust code without native linking. It does not execute Windows tests. The all-target check reports a pre-existing Windows-only unused import in `tests/workspace_recovery.rs`. A normal cross-build without those diagnostic overrides failed because macOS lacks `ml64.exe` and Windows C headers. Native Windows execution remains unverified.

Lean: the bounded model checked 4,681 traces / 18,056 transitions at depth 4, found no correct-model counterexample, and detected four broken variants. The correspondence worksheet, limits, costs and commands are in `2026-09-08-issue-342-resource-audit.md`. This is a model-only audit plus a reservation lemma, not a proof of OS or Tokio behavior.

## CI handoff

Local verification precedes PR creation. CI validates the final pushed SHA on the existing Linux runners, including Rust 1.88 and Lean. CI results belong to that SHA and are reported separately; this document does not claim those pending jobs passed.

The initial workflow run failed before creating jobs: the unquoted Rust filters `resource::` and `process::` ended with a colon followed by whitespace, which YAML treated as a mapping delimiter. Local `actionlint` reproduced the parse error at line 76. The added Windows job was removed; the final workflow contains no Windows runner. Workflow validation command:

```sh
docker run --rm --mount type=bind,source=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-342-resource-waits,target=/repo,readonly -w /repo rhysd/actionlint:latest -ignore 'label "cgroup-v2-delegated" is unknown' .github/workflows/ci.yml
```

This check passed. The exclusion covers only the existing custom self-hosted Linux label. The existing manual `non-linux-ci.yml` workflow is unchanged and was not dispatched.
