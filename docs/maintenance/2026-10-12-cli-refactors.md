# SQLite artifact names and CLI result handling

For maintainers reviewing [#826](https://github.com/HayaoSuzuki/hoimin/issues/826) and [#827](https://github.com/HayaoSuzuki/hoimin/issues/827), this record explains ownership boundaries and compatibility checks, based on main at `73b589630e0330e5410ab3b129ea1fe1bb840373`. These are internal ownership and control-flow changes, with no performance claim or compatibility migration. The package version floor remains 0.4.0.

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

Implementation self-review passes:

1. Compared the owned `run_from` and borrowed `run_with_io` code against the base: neither entry point nor its writer contract changed.
2. Traced each ordinary failure to the existing `command_result`: one stderr diagnostic and exit 2; ignored diagnostic-write failures still do not recurse.
3. Traced the plan success and failure sequence: configuration, preparation, JSON, newline, then the original output exit code. Converting errors to strings preserves their previous Display output.
4. Traced verify selection, dry-run preview, metrics assignment, and execution; no execution or metrics assignment moved into the preview branch.
5. Checked clap and completions separately: their output streams and exit behavior remain unchanged. The change adds no production helper, trait, dependency, or public contract.

Testing self-review passes:

1. Before changing result handling, ran `cli_config` (56 passed), `plan` (94 passed, including two added writer-failure cases), and `progress` (94 passed). These establish preservation of existing behavior rather than a bug-fix red/green claim.
2. After the refactor, CLI library tests passed again: 855 passed, 16 ignored. CLI configuration/help tests passed: 56 passed. Review and metrics integrations passed again: 5 and 19 passed.
3. Plan/verify tests passed: 94 passed. Both added tests use branch-free Arrange/Act/Assert structure and fixed-capacity standard-library writers. They distinguish failure during JSON serialization from failure after complete JSON when the final newline cannot fit. A strengthened diagnostic assertion and the existing preview writer-failure test passed in a final focused run: 3 passed.
4. Progress tests passed: 94 passed. Real CLI integration tests passed: 68 passed, 1 ignored, including owned-output, newline, interruption, and termination behavior. Ignored tests were not executed.
5. Final `cargo fmt --all -- --check`, `git diff --check`, and `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed, including after the assertion change. Windows execution is verified here; Unix/macOS execution remains for CI. No Python code changed, so the Python suite was not rerun.

The post-change command was `cargo test -p hoimin-cli --lib --test plan --test cli_config --test progress --test review_cli --test metrics_destinations --test run_e2e`. The final writer selection was `cargo test -p hoimin-cli --test plan output_failure`. Clippy checks all features, but these execution counts use default features; they do not claim contracts-feature execution.

The fresh independent reviewer found no Critical, Important, or Minor issues and confirmed the strengthened diagnostic assertion. The reviewer inspected the complete diff without editing or running heavy builds. Platform execution, non-Unicode filesystem fixtures, and competing-destination first-error fixtures were not independently executed; direct comparison preserves their construction and order. No runtime performance claim was assessed.

The focused software-design-philosophy assessment passed all eight diagnostic rows (10/10): shared naming knowledge has one owner, callers retain their policies, and existing error presentation handles more branches without a new framework. This score applies to these changes, not the whole repository. The documentation check passed all ten technical-documentation diagnostic rows (10/10) for the maintenance record and changed guide; no failed row or blocking factual issue remains.

Consulted the same compatibility and knowledge workflow sources as #826 and the saved-plan/CLI contracts. No public behavior, saved format, acceptance rule, formal-model guarantee, or release floor changed; existing concepts need no revision. No additional formal proof was warranted by this control-flow cleanup.

## Resource and documentation checks

Builds used two jobs and tests used two threads. A resource check during verification reported about 4.4 GiB of free physical memory. After all Rust checks finished, `cargo clean` removed 3,682 files (1.3 GiB) from the checked task-owned `.superpowers/runtime-improvements/target` path. The C: drive then had 182,464,925,696 bytes free (about 170 GiB). The ordinary workspace target was not cleaned.

The strict site build checks the changed maintainer guide and its link to this repository-only record. The record remains outside the publication allowlist. Historical audit results and ignored tests do not contribute to the execution claims here.
