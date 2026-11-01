# Issue #363: session result ownership

## Decision and implementation

We selected [#363](https://github.com/tokyogas-tech/hoimin/issues/363) for result
integrity after checking the open bug list against main `520a388`. The P2 issue
#338's `oom_kill` classification and regression tests already exist on that base;
this change does not close or otherwise resolve #338's remaining audit work.

The branch `fix/issue-363-session-ownership` has its own worktree at
`.worktrees/issue-363-session-ownership`. This branch includes the design and
this report alongside code and generated expectations.

`SessionHandler::persist` and `lookup` now require ownership of the requested
run. They return `session.persist.owner` or `session.lookup.owner` before any
database access if the handler lacks that ownership. We reused the existing
`finish` guard through `require_ownership`; the caller must use `begin` or a
successful `load` to acquire ownership. A successful `finish`, including an
incomplete finish, releases it. Owning a different run does not authorize access.

This changes error precedence for missing/completed runs and callers that skip
acquisition. We documented that contract on the public methods. We preserved
the schema, locking mechanism, authorized result replacement, and transaction
failure behavior. Existing corrupt-database and commit-failure tests now acquire
ownership before injecting faults, so they still exercise the database checks.

## Implementation evidence

Before the fix, the three new regression tests failed because unauthorized
operations returned success. After the fix, we observed:

| Check | Result |
| --- | --- |
| macOS `cargo test --workspace --all-features --quiet` | Exit 0; existing ignored tests remain ignored |
| macOS session handler, with and without contracts | 26 passed, 1 ignored per run |
| Linux nonroot session handler, contracts enabled | 26 passed, 1 ignored |
| macOS and Linux Lean adapter | 5 tests passed; 20 strict corpus cases matched |
| macOS and Linux workspace Clippy, all targets/features, warnings denied | Exit 0 |
| `cargo fmt --all -- --check`, `git diff --check` | Exit 0 |
| Independent review of implementation, tests, model, and depth option | No blocking findings |

The real SQLite tests compare whole logical database snapshots around rejected
inserts and replacement attempts. One test holds an external write transaction:
an unauthorized call must fail with the ownership error before trying to lock
the database. Handoff tests cover failed load, operations after finish both
before and after another handler acquires ownership, and successful reacquisition.

We ran Linux checks in `rust:1.98-bookworm` on aarch64 as UID/GID `501:20`, with
read-only source and a separate writable build mount. We did not run Windows,
the full Linux workspace suite, or the standalone Python test suite for this
Rust change. We did not poll GitHub Actions or dispatch extra CI jobs.

## Lean claim and correspondence

The existing finite session model covers two handlers and runs, acquisition,
result operations, release, and selected recovery/transaction interleavings.
We added two theorems: for any model state with a live handler that does not own
the requested run, persist and lookup reject with `notOwner` and preserve the
entire state. These proofs quantify arbitrary model states; they do not prove
the Rust implementation, OS locking, SQLite, or real crash behavior.

We first checked those theorems against the old model and observed proof
failures. After adding ownership guards, the model, proofs, and cases compiled.
We generated the JSONL from Lean and checked byte-for-byte freshness. The Rust
adapter executes public `SessionHandler` calls against isolated SQLite databases
and compares errors, results, and logical snapshots to the generated observations.
All 20 cases use the repository's strict mode; there is no separate report-mode
case set or unresolved mismatch for this change.

Two additional fixed cases cover foreign ownership and post-finish handoff.
The longest new trace has 14 events and runs regardless of search depth. Five
broken-model sensitivity checks detected their defects: atomicity, uniqueness,
boundary, unauthorized persist, and unauthorized lookup.

The bounded search used 31 events, depth 4, 118 states, and 961 transitions.
Its alphabet omits lookup; lookup coverage comes from the theorem, fixed corpus,
and broken witness. The bounded `safe` check does not establish the ownership
claim by itself. We preserved the default depth 8 and added `--depth 1..8` for
resource-limited execution. Depth 8 did not complete within the local deadline.

## Resource measurements and infrastructure

We ran Lean commands serially through `tools/lean_resource_guard.py`, with
20-second wall time, 768 MiB RSS, and 250 ms sampling. No limit was raised.

| Command | Elapsed ms | Peak RSS KiB | Exit |
| --- | ---: | ---: | ---: |
| Old model plus new ownership proofs (expected red) | 2170 | 716544 | 1 |
| Updated model compilation | 3754 | 723360 | 0 |
| Updated proofs compilation | 1091 | 711424 | 0 |
| Cases compilation | 538 | 601184 | 0 |
| Five sensitivity witnesses | 545 | 644128 | 0 |
| Depth-8 source generation attempt | 20040 | 717920 | 124 |
| Depth-4 corpus generation | 2683 | 655072 | 0 |
| Depth-4 freshness check | 1356 | 650048 | 0 |
| Depth-4 statistics | 2966 | 688368 | 0 |
| Invalid depth 9 | 2958 | 694800 | 2 (expected) |

The depth-8 timeout is an infrastructure limitation, not a semantic failure.
We lowered the bound and retained the proof and fixed-case checks. Earlier Rust
runs exposed stale corpus error expectations and an adapter error-code allowlist;
we regenerated expectations from Lean and added the two supported owner codes.

## Reproduction

From this worktree, with Rust 1.98 and the repository's Python environment:

```sh
uv sync --frozen
cargo test --workspace --all-features --quiet
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
HOIMIN_SESSION_ORACLE_CASE=result_operations_require_the_requested_runs_owner cargo test -p hoimin-cli --features contracts --test lean_session_oracle oracle_correspondence
HOIMIN_SESSION_ORACLE_CASE=result_operations_require_reacquisition_after_finish cargo test -p hoimin-cli --features contracts --test lean_session_oracle oracle_correspondence
```

For Lean, run these commands from `formal/HoiminOracle`. Each guarded command
must finish and its statistics must be inspected before the next command:

```sh
mkdir -p .lake/build/lib/lean/HoiminOracle
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-363-model.json -- lake -Kjobs=1 env lean -o .lake/build/lib/lean/HoiminOracle/SessionModel.olean HoiminOracle/SessionModel.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-363-proofs.json -- lake -Kjobs=1 env lean -o .lake/build/lib/lean/HoiminOracle/SessionProofs.olean HoiminOracle/SessionProofs.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-363-cases.json -- lake -Kjobs=1 env lean -o .lake/build/lib/lean/HoiminOracle/SessionCases.olean HoiminOracle/SessionCases.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-363-check.json -- lake -Kjobs=1 env lean --run SessionAuditMain.lean -- --depth 4 --check corpus/session-recovery.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-363-stats.json -- lake -Kjobs=1 env lean --run SessionAuditMain.lean -- --depth 4 --stats
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-363-sensitivity.json -- lake -Kjobs=1 env lean --run SessionAuditMain.lean -- --sensitivity
```

Use `--depth 4 --output corpus/session-recovery.jsonl` instead of `--check` to
regenerate. Do not hand-edit generated expectations.

## Counterexample ledger

| Schedule | Before fix | Required and observed after fix | Status |
| --- | --- | --- | --- |
| h0 begins r0; h1 persists into r0 | Insert/replacement could succeed | `.persist.owner`; unchanged database | Resolved, strict regression |
| h0 begins r0; h1 looks up r0 | Lookup could succeed | `.lookup.owner`; unchanged database | Resolved, strict regression |
| h0 finishes incomplete r0; h0 accesses results without reacquisition | Operations could succeed | Owner error before and after handoff; successful load restores access | Resolved, strict regression |

No unresolved ownership decision remains in this scope. The user authorized
implementation through PR creation; lookup follows the issue's suggested
resume-ownership contract. Deeper search and Windows execution remain unverified.
