# Coverage Matrix

| Area | Module | Static scan | Invariant trace | Dynamic evidence | Platform | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| analysis-output | `crates/hoimin-cli/src/analyzer/mod.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/analyzer/protocol.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/analyzer/rust.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/analyzer/rust_tests.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/analyzer/store.rs` | complete | pending | pending | pending | pending | |
| delivery | `crates/hoimin-cli/src/cli.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/fingerprint_inputs.rs` | complete | pending | pending | pending | pending | |
| delivery | `crates/hoimin-cli/src/lib.rs` | complete | pending | pending | pending | pending | |
| delivery | `crates/hoimin-cli/src/main.rs` | complete | pending | pending | pending | pending | |
| orchestration | `crates/hoimin-cli/src/metrics.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/plan.rs` | complete | pending | pending | pending | pending | |
| orchestration | `crates/hoimin-cli/src/process/mod.rs` | complete | pending | pending | pending | pending | |
| orchestration | `crates/hoimin-cli/src/process/output.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/progress/compare.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/progress/input.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/progress/mod.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/progress/render.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/report/human.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/report/json.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/report/jsonl.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/report/mod.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/resource/linux.rs` | complete | pending | limited | Linux cgroup | limited | Delegated Linux cgroup execution was not run on the macOS audit host. |
| isolation | `crates/hoimin-cli/src/resource/mod.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/resource/portable.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/resource/windows.rs` | complete | pending | limited | Windows | limited | Windows execution was not run on the macOS audit host. |
| persistence | `crates/hoimin-cli/src/session/mod.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/session/schema.rs` | complete | pending | pending | pending | pending | |
| orchestration | `crates/hoimin-cli/src/shell.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/target/fs.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/target/git.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/target/mod.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/copy.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/manifest.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/mod.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/mutation.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/reset.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/budget.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/candidate.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/config.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/contracts.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/effect.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/event.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/lib.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/machine.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/model.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/report.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/resume.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/target.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/telemetry.rs` | complete | pending | pending | pending | pending | |

## Static-scan triage notes

The six raw logs in `.audit/rust-codebase/scans/` contain 4,332 broad matches. Matches
under `crates/*/tests/`, the entirety of `analyzer/rust_tests.rs`, and matches inside
`#[cfg(test)]` modules were classified as test-only before reviewing production code.
The broad index pattern intentionally also matched attributes, slice types, and array
literals; those syntactic false positives were rejected rather than treated as risk by
count.

- `analyzer/{mod,protocol,rust,store}.rs`: parser offsets and slices originate from
  validated Ruff ranges or checked spool records; store `expect`s follow immediate
  initialization. Saturating/default conversions are explicit bounds policy.
  `analyzer/rust_tests.rs` is test-only.
- `cli.rs`, `lib.rs`, and `main.rs`: range defaults and nonzero defaults are
  construction invariants. Best-effort writes occur only while reporting an already
  selected CLI error; a failed Tokio runtime build cannot enter application error
  handling.
- `fingerprint_inputs.rs`, `plan.rs`, and `target/{fs,git,mod}.rs`: symlinks and
  root-relative paths are rejected, candidate lookups are preceded by validation, and
  fallbacks preserve lexical/user-facing values. No unchecked persistence operation
  remains.
- `metrics.rs`: temporary-file write, flush, sync, and persist failures propagate.
  Time conversion deliberately saturates; `Drop` restores only test process state.
- `process/{mod,output}.rs`: spawn, attach, wait, kill, join, and spool failures are
  surfaced or combined. The ignored second `start_kill` is a best-effort fallback after
  a bounded wait and `kill_on_drop(true)` remains armed.
- `progress/{compare,input,mod,render}.rs`: indexing is through validated collections
  or map entry APIs. The `unreachable!` arm follows the local two-input comparison
  state invariant.
- `report/{human,json,jsonl,mod}.rs`: serialization and write errors are converted to
  `EffectFailed`; temporary JSON storage is owned and checked.
- `resource/mod.rs` and `resource/portable.rs`: unsafe blocks are narrow FFI calls with
  ownership/lifetime comments; termination errors propagate from the explicit path.
  The Windows attach race remains as `RUST-001`.
- `resource/linux.rs`: cgroup paths are canonicalized and subtree walks reject
  symlinks. Kill/reap/cleanup errors are retained in retryable pending-cleanup values;
  ignored operations occur only in `Drop` fallback paths. Linux-only execution remains
  a platform evidence gap, not an additional static lead.
- `resource/windows.rs`: Win32 handles use an owning wrapper and checked API results;
  integer conversions are bounded by API constants/layout sizes. The platform was not
  executable on this host; the pre-assignment race is recorded as `RUST-001`.
- `session/{mod,schema}.rs`: state transitions use transactions and commit errors
  propagate; migrations advance `user_version` inside the same transaction. Schema
  `unwrap`s are confined to tests.
- `shell.rs`: task joins, cancellation, deadline, cleanup, and close failures are
  drained or combined. Completion sends are ignored only after receiver shutdown, and
  metrics diagnostics are explicitly best-effort so they do not replace the run result.
- `workspace/{copy,manifest,mod,mutation,reset}.rs`: canonical roots, component-wise
  path validation, symlink rejection, snapshots, and post-reset comparison bound file
  access. Cleanup failures remain retryable and prevent accounting release; defaults
  represent unavailable advisory metadata or absent environment values.
- `hoimin-core` modules: matches are declarative attributes/slice types or checked,
  saturating conversions. `machine.rs` collection indexing follows registered-effect
  and worker-state checks; `resume.rs` casts encode platform-bounded lengths into the
  stable fingerprint format; `target.rs` fallbacks implement normalization policy.
  No unsafe block or cleanup side effect exists in core production code.
