# Default operator selection review and validation

Scope: main `5ea8a9c39a54d411b5f087fcc37558e395b86d6f` plus this worktree.
The user chose the seven-operator rollout on 2026-10-07. Design and plan each
contain five self-review passes; the implementation and test passes follow.

## Implementation self-reviews

1. Exact membership: inspected the default diff and independent literal CLI list.
   Only the seven approved IDs are added; the registry remains 69 and all_legacy
   remains 43. The six other new analyzers stay opt-in.
2. Entry points: traced RawRunConfig normalization and Rust Default. Replaced the
   raw fallback with Default; explicit selection still begins empty and exclusions
   still apply last. Family selectors and empty-set validation are unchanged.
3. Fixture intent: classified full-suite failures by extra candidate, changed
   first candidate, or incomplete mutation budget. Historical exact-record/Lean
   fixtures pin legacy selection locally. Most analyzer helpers retain Default;
   current depth, preprocessing, focused profile and CLI expectations include additions.
4. Persistence: inspected PlanConfig's required serialized operators and the verify
   rediscovery path. The public test writes a simulated legacy plan and verifies
   it under current defaults. No schema migration or implicit expansion was added.
5. Resource and rollout effects: the seven exclusions reproduce old candidates
   including IDs/order in both project trials. Integer neighbors can substantially
   increase counts. CLI help/README document current defaults and opt-out; no
   collector, limit implementation or exit-code policy was changed.

## Test self-reviews

1. RED evidence: before changing production, operator_selection had 14 passes and
   two expected failures (43 versus 50). After the policy edit all 16 passed.
2. Exactness: core tests check legacy subset, exactly seven added names, default
   size, omitted raw selectors, exclusions, explicit override and serialized legacy
   selection. CLI's literal 50-ID expectation is independent of the implementation.
3. Public observations: the new CLI fixture exercises all seven operators. Each
   exclusion preserves the other candidate descriptors and ordered IDs. Rank and
   sequence are intentionally removed only for this comparison because removal
   renumbers them. Full default-versus-explicit and legacy-versus-opt-out comparisons
   retain every field. Legacy verification checks baseline, survivor and source bytes.
4. Boundaries: the default analyzer test checks Full/Focused behavior, skips the
   bare print in Focused, compares the capped prefix and observes cancellation.
   Existing per-operator line/symbol/limit/cancellation tests now assert default
   membership for the promoted operators and retain their post-exclusion assertions.
5. Runtime interpretation: disk/logging/metrics fixtures with max-mutants 1/2
   acquired additional integer mutants and correctly returned incomplete (4).
   Those tests now select their intended arithmetic operator, or use a one-candidate
   source. No runtime assertions were weakened to accept incomplete runs. Tests that
   exercise default profiles instead enumerate the additional candidates.

## Lean audit

Mode: `model-only` for every theorem and fixed witness. No generated Rust corpus,
no exhaustive search, no claim that Lean proved the Rust parser or serde code.
Theorems quantify over membership functions on Nat; no search bound is assumed.
The wall-clock deadline is 20 seconds; no enlarged heartbeat/recursion limit,
`sorry`, `axiom`, or `native_decide` is used.

| Premise/observation | Lean representation | Production configuration / public observation | Evidence |
| --- | --- | --- | --- |
| Omitted selectors use legacy plus promotions | `defaults`, `select … none` | CLI omission; emitted normalized_config.operators | core selection tests, public default test |
| Explicit selectors override defaults | `select … (some chosen)` | `--operators`; emitted set and candidates | explicit selection tests |
| Exclusions win | final Boolean conjunction | `--exclude-operators`; remaining candidates | seven individual public exclusions |
| Removing promotions restores legacy | disjoint legacy/promoted premise | exact 43/50 membership and seven opt-outs | literal CLI list, core set comparison, project trials |
| Saved selection stays frozen | `reload saved = saved` | required PlanConfig operators; verify rediscovery | serde roundtrip and simulated legacy CLI plan |

The last model function deliberately abstracts away serialization, validation,
source hashes and ranking. Its theorem alone provides no persistence guarantee;
those implementation paths are inspected and exercised separately.

The six theorems cover exclusion precedence, explicit override, promoted inclusion,
legacy retention, disjoint opt-out recovery and persistence identity. Two fixed
witnesses reject the risky alternatives of unioning an explicit selection that excludes operator 0
with defaults, or restoring a promoted operator after excluding it. The explicit witness selects operator 1 but excludes operator 0; production also
rejects empty normalized selections, as covered by the existing core tests.
No semantic counterexample to the stated model claims was found. The witnesses
are closed evaluations, not an exhaustive enumeration or production comparison.

Command (cwd `formal/HoiminOracle`): `lake env lean DefaultOperatorSelection.lean`,
invoked through Python subprocess with timeout=20; passed, no diagnostics.

## Project trials

See [project-trials.json](project-trials.json): copied installed packaging.version
(39,040 bytes) and iniconfig/__init__.py (7,497 bytes) into isolated temporary roots,
then invoked the public plan CLI with max-candidates 10,000 and /usr/bin/true as the
saved command. Plan does not execute that command. These are discovery observations,
not mutation execution or a performance benchmark. No project-wide claim is made.

| Source | Legacy | Default | Seven opt-outs | Truncated |
| --- | ---: | ---: | ---: | --- |
| packaging.version | 248 | 476 | 248 | no |
| iniconfig | 24 | 34 | 24 | no |

Each opt-out candidate array equals the legacy array exactly. SHA256 and single-run
CLI timings are recorded in the JSON; subsecond timings are not used as performance
acceptance criteria. Temporary trial roots were removed after each file.

## Independent review

The reviewer inspected the diff and new files without building or editing. It found
no actionable correctness/compatibility defects, confirmed both entrypoints and
persisted selection behavior, and limited its conclusion to static review pending
validation. It did not claim broad real-project runtime coverage.

## Final checks

On macOS/aarch64 with the pinned Rust 1.98.1 and CPython 3.14.7:

- `cargo test --offline --workspace --no-fail-fast`: exit 0; 2,680 passed,
  0 failed, 22 ignored across 142 test targets/doc-test summaries. Ignored fixtures
  and benchmarks remain ignored; this is not Windows/Linux execution evidence.
- `cargo test --offline -p hoimin-core --features contracts`: exit 0;
  343 passed, 0 failed, 2 ignored.
- Focused public CLI, plan, disk, operator and telemetry reruns passed after fixture
  corrections. The final workspace run also passed all of these targets.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`:
  exit 0, matching repository CI. An exploratory invocation without --all-features
  hit the pre-existing too_many_lines lint in lean_report_sequence_oracle.rs:372;
  that module is disabled by the contracts feature in CI. It was not modified.
- `cargo clippy --offline --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`:
  exit 0.
- Workspace and vendored-parser `cargo fmt ... -- --check`: exit 0.
- Lean selection model: exit 0 under a 20-second deadline, including the two explicit
  broken-rule inequality witnesses.
- Independent final delta review: no actionable findings; no builds/tests claimed.

Builds used CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0 and
CARGO_INCREMENTAL=0. The full suite initially exposed old candidate/count/budget
assumptions; the changes and reasons are recorded above, rather than treating
those failed runs as successful evidence. The first draft of the CLI test also
used an incorrect PlanManifest import and was corrected before behavioral execution.

The affected OKF pages are design/analyzer.md and references/audit-documents.md.
Current default claims are separated from historical opt-in reports. The catalog
records hashes of this change's design/review sources; final catalog checks cover
YAML/reserved files, new source hashes, links, footnotes and index reachability.


Catalog validation passed for all 29 Markdown files: YAML/reserved-file rules and
root-index reachability. Both changed catalog pages have unique source IDs, matching
new source hashes/footnotes, and existing targets for their document links. The final
preview-policy test was rerun after extracting its options variable for the line-count lint.
