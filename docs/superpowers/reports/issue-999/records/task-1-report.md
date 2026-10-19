# Task 1 report: production arithmetic extraction

Status: **DONE_WITH_CONCERNS** (only local cargo-deny execution remains unavailable; parent Task3 owns final dependency audit). Base: `7110f2ef`; worktree: `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-999-creusot-budget-proofs`.

## Implementation

Extracted the production arithmetic and value types into `hoimin-budget-math` with the exact requested public API. Core imports the new crate directly; CLI has its own dependency for its timeout assertion. Removed the previous core projection module and reexports. The production projection/auto-timeout calculation bodies are unchanged. No proof annotations, tools, compatibility wrappers or duplicate arithmetic were added.

`NonZeroDuration::new(Duration) -> Option<Self>` validates positivity. Core conversion sites preserve each existing named `ConfigError::InvalidLimit`. Optional serde changes only serialization implementations/derive attributes; its zero-duration error retains `invalid zero or overflowing limit: duration`. Enum and Duration JSON representations remain unchanged.

Updated workspace graph, root/fuzz locks, release version rewriting, SBOM required members and generator sidecar cleanup, actual sdist member checks and graph permutations. Added a narrow internal 0.13.x Elastic-2.0 exception in deny.toml. Reverted unrelated zerocopy/zerocopy-derive updates produced by lock regeneration. Existing maturin automatic local-path inclusion was verified against an actual sdist; pyproject.toml needs no redundant explicit include.

## Commands, RED evidence and resolved failures

Every Rust command uses `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-issue999-target`; only one heavy Rust build ran at a time. No Creusot installation was attempted. Python uses `/Users/hayao/RustroverProjects/hoimin/.venv/bin/python`; final runs also set `UV_CACHE_DIR=/tmp/hoimin-issue999-uv-cache` and put the shared venv bin directory first on PATH.

1. Before production edits: `cargo test -p hoimin-core --test budget_projection --test config --test machine --test lean_top_budget_projection_oracle --locked` exited 101 because this repository has `config_json`, not a `config` test target. Corrected command: `cargo test -p hoimin-core --test budget_projection --test config_json --test machine --test lean_top_budget_projection_oracle --locked`. Output: **8 + 13 + 2 + 110 = 133 passed**, no failures. [Full corrected baseline output](task-1-evidence/baseline.log).
2. New crate tests written before implementation: `cargo test -p hoimin-budget-math --all-features --offline` exited 101 with E0432 for the five absent requested API items. This is **structural API RED**, not evidence that an incorrect arithmetic body was rejected. [RED output](task-1-evidence/red.log).
3. Before updating Python production tooling: `python -m pytest tests/test_sbom.py::test_budget_math_identity_is_required tests/test_release.py::test_version_updates_manifests_and_both_lockfiles -q` produced **4 failed**: both release versions left the new crate at 0.13.0 and both corrupted new-crate SBOM cases failed to raise. [Behavioral RED output](task-1-evidence/python-red.log).
4. First focused Python regression run: **9 failed, 115 passed**. Two release-workflow cases were blocked by sandbox writes to the default uv cache; reran with task-owned cache/PATH. Two cleanup cases found `tools/capture_sbom.py`'s explicit old three-member list; production list updated. Five preexisting corruption/order cases depended on fixture component positions; appended the new component instead of prepending it. [Initial failures](task-1-evidence/python-green.log).
5. First complete Python suite: **5 failed, 1095 passed, 11 skipped**. `test_graph_order_does_not_change_validation` still permuted only the original four components/five dependency nodes; changed to five/six. Four `tests/test_lean_resource_guard.py` cases (`test_propagates_normal_child_exit`, `test_rejects_root_exit_that_leaves_a_descendant`, `test_terminates_process_group_at_timeout`, `test_terminates_process_group_at_rss_limit`) returned monitor-error exit126 because sandbox blocks ps. Focused rerun with authorized sandbox escalation: **8 passed**; complete rerun: **1100 passed, 11 skipped**. [Initial complete run](task-1-evidence/pytest-all.log), [focused follow-up](task-1-evidence/python-followup.log), [final complete run](task-1-evidence/pytest-all2.log).
6. First complete Rust suite stopped at `abstract_set_provider`: `provider_set_pairs_generate_evaluable_annotations` and `provider_set_public_run_survives_annotation_evaluation` could not execute the worktree-relative `.venv/bin/python`. Added an ignored worktree-local `.venv` symlink to the preexisting shared environment, then reran the entire workspace suite with required process-monitor access. No personal environment files were changed. [Initial complete run](task-1-evidence/workspace.log).
7. First `ty check` invocation selected system Python and reported 40 unresolved-import/decorator follow-on diagnostics. Explicit `--python /Users/hayao/RustroverProjects/hoimin/.venv/bin/python` fixed environment selection; focused typecheck passed without code changes.

## GREEN verification obtained

- `cargo test -p hoimin-budget-math --all-features --offline`: **29 arithmetic + 6 serde tests passed**. [Output](task-1-evidence/green.log).
- `cargo test -p hoimin-budget-math --no-default-features --locked`: **29 passed**. [Output](task-1-evidence/no-serde.log).
- `cargo tree -p hoimin-budget-math --no-default-features -e normal --locked`: only the new root crate; no normal dependency is enabled.
- `cargo test -p hoimin-core --test budget_projection --test config_json --test machine --test lean_top_budget_projection_oracle --test lean_timeout_limit_oracle --test target_policy --test resume_policy --all-features --locked`: **197 passed**, including Lean corpus correspondence and runtime contracts. [Output](task-1-evidence/core-green.log).
- `cargo check --workspace --all-features --locked`: exit0, stable repository toolchain; finished in 21.07s. [Output](task-1-evidence/check.log).
- `python -m pytest tests/test_sbom.py tests/test_release.py tests/test_release_version_contracts.py tests/test_sdist.py -q`: **124 passed in 14.29s**, actual maturin sdist included. [Output](task-1-evidence/python-green2.log).
- Full `python -m pytest -q` with process-monitor access: **1100 passed, 11 skipped in 41.92s**.
- `cargo fmt --all -- --check` and vendored parser formatting check: exit0.
- `ruff format --check` / `ruff check --no-fix` on changed Python files: exit0.
- `ty check --python /Users/hayao/RustroverProjects/hoimin/.venv/bin/python` on changed Python files: exit0. [Output](task-1-evidence/ty.log).
- `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1 --no-deps --locked --offline`: exit0; separate fuzz lock is consistent.
- `git diff --check`: exit0.

## Python mutation verification

Applied the hoimin-mutation-testing skill. A read-only plan found 12 candidates using the newly built working-tree binary. Kept jobs1, maximum workspace8GiB, minimum free10GiB, profile focused, changed tools source, pyproject fingerprint, test argv and best-effort macOS memory policy fixed. The selected candidate was `m1_0bafe849a5b1baa74c096010b4e8e9470105021c63d8f701cba965f6a9d173ca`, replacing the new `"hoimin-budget-math"` member in `tools/capture_sbom.py` with `""`.

Plan command:

```sh
/tmp/hoimin-issue999-target/debug/hoimin plan --root . --source tools --changed --profile focused --jobs 1 --max-workspace-size 8GiB --min-free-space 10GiB --allow-best-effort-memory --fingerprint-include pyproject.toml -- /Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m pytest tests/test_sbom.py tests/test_release.py tests/test_release_version_contracts.py tests/test_sdist.py -q
```

Verification command (PLAN is an exact mktemp-owned path):

```sh
/tmp/hoimin-issue999-target/debug/hoimin verify "$PLAN" --candidate m1_0bafe849a5b1baa74c096010b4e8e9470105021c63d8f701cba965f6a9d173ca --format json
```

First attempt stopped with `workspace.original.changed: original changed: deny.toml` because I edited the dependency policy while the worker was taking its snapshot. Stopped, diagnosed, cleaned the exact temporary directory, regenerated the plan after freezing edits, then reran. Final JSON: baseline Exit0/15766ms; mutant Exit1/14840ms; killed1, survived0, timeout0, error0, not_run0, inconclusive0; complete:true; exit_code:0. Peak owned54247590 bytes; minimum available103256248320 bytes; execution cleanup clean, no remaining root. All immutable mktemp directories had cleanup traps and were removed; no mutation manifests or reports remain outside the report's summary. No source mutations were retained.

## Reviews and concerns

Five distinct implementation self-reviews and five testing self-reviews are recorded in `docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md`. Findings/fixes include preserving constructor error identity, serde diagnostic/shape, no-feature dependency isolation, sidecar cleanup, unrelated lock drift, fixed fixture indexes and graph permutation counts.

`cargo deny --version` exits101 (`no such command: deny`). The scoped policy exception was updated, but actual cargo-deny checks are pending the parent Task3 dependency audit. No Creusot/proof/cross-target/CI success is claimed by Task1. The initial API RED was compilation-level only, as explicitly distinguished above. No actual release/wheel publication was attempted.

Disk before heavy work:108GiB free. During/following full workspace build:96GiB free; shared task-owned target approximately12GiB. No other target or worktree was cleaned. Parent retains this target for later tasks.

## Changed tracked files (before staging new files)

```text
M	Cargo.lock
M	Cargo.toml
M	crates/hoimin-cli/Cargo.toml
M	crates/hoimin-cli/tests/cli_config.rs
M	crates/hoimin-core/Cargo.toml
D	crates/hoimin-core/src/budget_projection.rs
M	crates/hoimin-core/src/config.rs
M	crates/hoimin-core/src/lib.rs
M	crates/hoimin-core/src/machine.rs
M	crates/hoimin-core/src/resume.rs
M	crates/hoimin-core/tests/budget_projection.rs
M	crates/hoimin-core/tests/config_json.rs
M	crates/hoimin-core/tests/lean_timeout_limit_oracle.rs
M	crates/hoimin-core/tests/lean_top_budget_projection_oracle.rs
M	crates/hoimin-core/tests/resume_policy.rs
M	crates/hoimin-core/tests/target_policy.rs
M	deny.toml
M	fuzz/Cargo.lock
M	tests/sbom_fixtures.py
M	tests/test_release.py
M	tests/test_sbom.py
M	tests/test_sbom_properties.py
M	tests/test_sdist.py
M	tools/capture_sbom.py
M	tools/release.py
M	tools/sbom.py
```

New files: `crates/hoimin-budget-math/{Cargo.toml,src/lib.rs,src/duration.rs,src/projection.rs,tests/arithmetic.rs,tests/serialization.rs}` and the durable review document named above.

## Final workspace test / normal hooks / commit

Full `cargo test --workspace --all-features --locked` after resolving the local Python path: **2916 passed, 0 failed, 33 ignored**, across 171 test/doctest summaries; command exit0. Includes the five compile-fail doctests. [Final complete Rust output](task-1-evidence/workspace2.log).

`PREK_HOME=/tmp/hoimin-issue999-prek CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-issue999-target prek run --all-files` exited0:

```text
cargo fmt (workspace)....................................................Passed
cargo fmt (vendored parser)..............................................Passed
cargo clippy (workspace).................................................Passed
cargo clippy (vendored parser)...........................................Passed
```

Staged only the 32 task-owned files, checked `git diff --cached --check`, and committed with normal hooks enabled using the same shared target/jobs/cache environment. All four hooks passed again.

Commit: `b63329d725e3eed77ae77603ffda1570644bc93f` — `refactor: extract production budget arithmetic into internal crate`.

`git status --short` after commit: empty (clean). The ignored worktree-local `.venv` link and `.superpowers` evidence/report remain available for the parent's subsequent tasks. Final free space:96GiB; task-owned shared target approximately12GiB. The projection file was detected as a 100% rename, corroborating unchanged arithmetic.

[All-files hook output](task-1-evidence/hooks.log), [normal-hook commit output](task-1-evidence/commit.log).

Final committed files:

```text
M	Cargo.lock
M	Cargo.toml
A	crates/hoimin-budget-math/Cargo.toml
A	crates/hoimin-budget-math/src/duration.rs
A	crates/hoimin-budget-math/src/lib.rs
R100	crates/hoimin-core/src/budget_projection.rs	crates/hoimin-budget-math/src/projection.rs
A	crates/hoimin-budget-math/tests/arithmetic.rs
A	crates/hoimin-budget-math/tests/serialization.rs
M	crates/hoimin-cli/Cargo.toml
M	crates/hoimin-cli/tests/cli_config.rs
M	crates/hoimin-core/Cargo.toml
M	crates/hoimin-core/src/config.rs
M	crates/hoimin-core/src/lib.rs
M	crates/hoimin-core/src/machine.rs
M	crates/hoimin-core/src/resume.rs
M	crates/hoimin-core/tests/budget_projection.rs
M	crates/hoimin-core/tests/config_json.rs
M	crates/hoimin-core/tests/lean_timeout_limit_oracle.rs
M	crates/hoimin-core/tests/lean_top_budget_projection_oracle.rs
M	crates/hoimin-core/tests/resume_policy.rs
M	crates/hoimin-core/tests/target_policy.rs
M	deny.toml
A	docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md
M	fuzz/Cargo.lock
M	tests/sbom_fixtures.py
M	tests/test_release.py
M	tests/test_sbom.py
M	tests/test_sbom_properties.py
M	tests/test_sdist.py
M	tools/capture_sbom.py
M	tools/release.py
M	tools/sbom.py
```



## Parent-requested follow-up: independent wave oracle

After the initial commit, the parent corrected the plan and brief: the initially requested quotient/remainder oracle was structurally identical to production. Implemented the corrected requirement in the test only:

- Wave property now checks the minimal-covering characterization `selected <= waves * jobs` and, if positive, `(waves - 1) * jobs < selected`, with u128 products.
- Capacity oracle computes wave count independently with widened additive ceiling `(selected as u128 + jobs as u128 - 1) / jobs as u128`; division threshold still prevents multiplication overflow.
- Updated durable testing review and included the parent's plan correction in the additional commit. No production implementation changed.

Verification: `cargo test -p hoimin-budget-math --all-features --locked` passed all 29 arithmetic and 6 serde tests. `cargo fmt --all -- --check` passed. Initial focused Clippy run identified the deliberately explicit additive ceiling as `clippy::manual_div_ceil`; added a statement-local allowance with its independent-oracle rationale. Final `cargo clippy -p hoimin-budget-math --all-targets --all-features --locked -- -D warnings` passed (0.19s). See [initial focused tests](task-1-evidence/oracle-followup.log), [initial lint diagnostic](task-1-evidence/oracle-clippy.log), [final tests](task-1-evidence/oracle-followup2.log), [final focused lint](task-1-evidence/oracle-clippy2.log). The previously passing whole-workspace, Python and mutation runs were not repeated, as explicitly requested by the parent for this test-only follow-up.

The plan-file modification observed after the initial clean status was the parent's requested correction; it is deliberately included in this follow-up, not an unrelated leftover.

Follow-up commit: `e5ce0c6d4890e0ba4034ba5851fb85b521d3cd5b` — `test: make budget wave expectations independent of production`. All four normal pre-commit hooks passed (workspace/vendor formatting and Clippy), with shared target/jobs=2. [Normal-hook commit output](task-1-evidence/oracle-commit.log). Final `git status --short` is empty; both Task 1 commits are retained.
