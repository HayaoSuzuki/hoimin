# SQLite artifact names and CLI result handling

This record covers [#826](https://github.com/HayaoSuzuki/hoimin/issues/826) and [#827](https://github.com/HayaoSuzuki/hoimin/issues/827), based on main at `73b589630e0330e5410ab3b129ea1fe1bb840373`. These are internal ownership and control-flow changes, with no performance claim or compatibility migration. The package version floor remains 0.4.0.

## SQLite companion paths (#826)

The internal `artifacts` module owns SQLite companion names. Its helper appends suffixes to native OS strings without resolving paths or accessing the filesystem. Each caller still owns canonicalization, aliases, file identity, symlink policy, and validation order.

Design self-review passes:

1. Checked ownership: a neutral artifact helper avoids coupling review input to session persistence.
2. Checked the API: named WAL, SHM, and journal paths let callers choose their existing order without configurable policies.
3. Checked path spelling: suffixes append to the entire OS string; extensions and non-Unicode names are preserved.
4. Checked safety boundaries: canonical resolution, Windows configured aliases, and protected-file checks stay in their callers.
5. Checked scope: no connection, schema, public API, lock-directory, or destination-policy changes are needed.

Implementation plan self-review passes:

1. Traced all three production naming sites: session files, metrics configured aliases, and review-store validation.
2. Recorded each order: session database/WAL/SHM/journal; metrics aliases WAL/SHM/journal; review database/journal/WAL/SHM.
3. Chose existing session, metrics, and review tests as behavioral protection instead of a test copying the suffix table.
4. Kept the helper private and dependency-free; no standalone abstraction framework or public configuration.
5. Limited this branch to #826 and this record so #827 remains independently reviewable above it.

Implementation self-review passes:

1. Compared all three edited callers with the baseline: suffixes and returned/validated path order match.
2. Confirmed `SessionArtifacts::resolve`, lock trees, alias inspection, and deduplication are unchanged.
3. Confirmed review still checks the database first and journals before WAL/SHM, preserving first-error behavior.
4. Checked native-string construction and allocations: no lossy conversion, canonicalization, or unused database clone in the helper.
5. Checked module visibility and the complete diff: no public API, SQLite schema, dependency, or unrelated source changes.

Testing self-review passes:

1. Ran the baseline CLI library tests: 855 passed, 16 ignored; the same counts pass after the refactor.
2. Ran metrics destination integration tests before and after: 19 passed, including Windows aliases, companion protection, and inactive-name acceptance.
3. Ran review integration tests before and after: 5 passed; CLI library tests also cover protected sources, hard links, and a protected journal.
4. Ran session integration tests before and after: 33 passed, 1 ignored; the ignored test remains outside this execution scope.
5. Confirmed `cargo fmt --all -- --check`, `git diff --check`, CLI-library Clippy with `-D warnings`, and the strict documentation build pass. Tests ran on Windows; Unix branches require CI coverage.

Commands used `cargo test -p hoimin-cli --lib --test metrics_destinations --test review_cli --test session_handler` after the change and the equivalent library/integration selections before it. No suffix-table mirror test was added. Build and test parallelism was limited to two, with incremental compilation and debug info disabled in the task-owned target directory.

Consulted `docs/compatibility.md`, `docs/okf-workflow.md`, and the session/report and survivor-review knowledge concepts. Their acceptance and persistence contracts are unchanged; no concept or formal-model revision is needed for this naming-only cleanup. The maintainer ownership guide links this record.

## CLI result handling (#827)

Reuse `command_result` for borrowed-writer command results. Flatten fallible plan preparation and serialization with `?`, retaining its manifest exit code and final newline. Preserve clap help/usage handling and the owned, deadline-aware `run_from` path.

Design self-review passes:

1. Checked writer ownership: borrowed synchronous writers and owned deadline-aware writers are different contracts and must remain separate.
2. Checked error presentation: ordinary command errors print once to stderr and return 2; clap retains its own stream and code.
3. Checked success codes: plan returns the generated output's code, while dry-run verify returns 0.
4. Checked serialization: manifest JSON must finish before writing its newline, and either write failure must return 2.
5. Checked abstraction cost: reuse the existing helper without adding wrappers, traits, or a command framework.

Implementation plan self-review passes:

1. Keep `run_from` unchanged, including its diagnostic timeout and owned execution routes.
2. Consolidate only repeated ordinary-result handling in `run_parsed_with_io`.
3. Keep verify preparation, preview, metrics assignment, and execution in their existing order.
4. Inspect existing CLI, plan, progress, review, and writer-failure tests; add only missing observable contract coverage.
5. Run focused integration tests and strict formatting/Clippy checks with two build jobs in a dedicated target directory; clean that directory after verification.

Execution and verification will be recorded on the second stack branch.
