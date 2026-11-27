# Changed-target range composition Lean audit

## Verdict

Issue #313 found one same-premise Rust mismatch. A changed candidate inside an
explicit symbol selector was absent from public `plan`. Target resolution had
correctly resolved `a:Widget.run` to `pkg/a.py`, but transported only
`Widget.run`; the analyzer required a `module:qualname` string and rejected every
qualname-only selector.

The retained regression creates a Git repository where changed lines exist in
both `Widget.run` and another function, then runs public
`plan --changed --symbol a:Widget.run`. Before the repair it returned no
candidates. The minimal repair makes the analyzer accept the resolver's owned
qualname form while retaining support for module-qualified internal requests.
It does not change the public schema or target representation.

## Formal contract

Lean models parsed semantic change facts rather than unified-diff bytes. A fact
records its kind, source and destination paths, destination line ranges, and
current line count. Range normalization is specified extensionally as membership
in valid positive one-based inclusive ranges. `normalizedLines` materializes that
set within an explicit bound as an ordered, duplicate-free list. Invalid ranges
vanish and overlapping or adjacent ranges have the same canonical line-list
representation as their merged form. Rust tests separately pin concrete range
materialization.

The kernel-checked modules prove:

- bounded normalization preserves membership and its canonical line-list output
  is idempotent;
- changed/explicit intersection is commutative at observation level and a
  subset of both inputs;
- intersection cannot create a path absent from the changed facts;
- deletion and binary exclusion dominate positive facts;
- rename selection uses the destination and cannot select a distinct source;
- an untracked selection is bounded by `1..currentLineCount`;
- empty explicit selection is the identity for changed selection.

The external consumer imports only the model and proof modules. Cases,
sensitivity checks, corpus generation, and Rust fixtures stay outside the proof
dependency boundary.

## Correspondence worksheet

The closed corpus contains eleven rows. Each row is rendered from a typed Lean
`AuditCase`; `eligible_lines` is computed by enumerating `CombinedEligible`, not
stored as an expected literal. Rust looks up each strict row by ID and compares
the public observation with those generated lines.

| Mode | Rows | Observation |
| --- | ---: | --- |
| `strict` | 8 | real Git fixture and public plan candidates |
| `internal-fixture` | 1 | owned parser-isolation coverage |
| `model-only` | 1 | non-UTF-8 logical path not representable publicly |
| `infrastructure-error` | 1 | Git/setup failure classification only |

Strict cases cover contiguous modified lines, explicit line intersection,
symbol intersection, rename destination attribution, deleted and binary
exclusion, an unterminated untracked last line, and `--diff-base` composition of
committed HEAD plus worktree edits. Candidate observations check path, line,
source byte span, original/replacement/operator, a source-derived BLAKE3 file
hash, symbol where applicable, and a stable ID recomputed only after those
fields are independently validated. The deletion/binary fixture also inspects
owned `ResolveGitChanges` output and raw Git name/numstat premises, so zero
candidates cannot hide a leaked target.

Git parsing remains Rust evidence. Existing property tests generate hostile
zero-context diff bodies. A named internal regression corresponding to the
`hostile-parser` row shows that structural-looking body lines in one section do
not contaminate the later valid Python section. Integration tests pin
destination hunk coordinates, rename configuration, binary numstat, deletion,
and staged/unstaged cancellation. These are not presented as Lean grammar proofs.

## Sensitivity and bounded cases

All ten explicit faulty transports are distinguished from their canonical
observations: failure to merge adjacency,
merging across a one-line gap, old-side coordinates, deleted/binary retention,
source-side rename attribution, untracked final-line loss, union instead of
intersection, dropped symbol restriction, pre-normalization path membership,
and contamination of a later valid section. Eight fixed semantic cases and ten
sensitivity families run within the recorded bound of two facts and two ranges.
The generated bounded range-set enumeration evaluates preservation and
idempotence across 1,365 membership states; `--stats` fails if any state fails.
Enumeration is evidence, not proof.

## Counterexample ledger

| Field | Observation |
| --- | --- |
| Premise | changed line 3 is inside `pkg/a.py`, symbol is `Widget.run` |
| Explicit selector | `a:Widget.run` resolves to `pkg/a.py` |
| Lean | combined observation at line 3 is eligible |
| Rust before repair | public plan has zero candidates |
| Cause | resolver transports `Widget.run`; analyzer rejected strings without `:` |
| Repair | interpret qualname-only selectors at the already resolved target path |
| Regression | `public_plan_preserves_symbol_restrictions_under_changed_intersection` |

## Resource measurements

Each retained Lean command ran alone with a 20,000 ms deadline, a 786,432 KiB
root-plus-descendant RSS ceiling, and 250 ms sampling.

| Command | Elapsed ms | Peak RSS KiB | Exit / reason |
| --- | ---: | ---: | --- |
| proof module direct compile | 1,630 | 573,872 | 0 / `child_exit` |
| external proof consumer | 565 | 54,512 | 0 / `child_exit` |
| ten sensitivity families | 292 | 224 | 0 / `child_exit` |
| eight fixed cases | 288 | 2,672 | 0 / `child_exit` |
| corpus freshness | 291 | 2,112 | 0 / `child_exit` |

No retained command reached either limit. Peak RSS remained 212,560 KiB below
the ceiling.

## Exclusions

Git merge-base correctness, Git rename detection itself, a byte-for-byte diff
grammar proof, candidate ranking after eligibility, and filesystem races after
Git observation remain outside this audit.

## Verification commands

```text
lake build
lake env lean HoiminOracle/ChangedTargetProofs.lean
lake env lean /tmp/hoimin-changed-target-proof-consumer.lean
lake exe generate_changed_target -- --cases
lake exe generate_changed_target -- --sensitivity
lake exe generate_changed_target -- --stats
lake exe generate_changed_target -- --check corpus/changed-target-composition.jsonl
cargo test -p hoimin-core --test target_policy --all-features
cargo test -p hoimin-cli --test target_handler --all-features
cargo test -p hoimin-cli --test lean_changed_target_oracle --all-features
cargo test -p hoimin-cli --test plan --all-features
cargo test --workspace --all-features
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

## Diff selection configuration

The configuration model covers `--changed`, `--changed-context`, and
`--diff-base` alongside the four explicit selector types. Lean proves that
accepted changed selection requires a source and that twice the maximum
accepted context fits a signed 32-bit value.

The 386 fixtures distinguish three boundaries. The CLI rejects an explicit
context without `--changed`, including zero, and checks the numeric range
before constructing run settings. Raw configuration treats omitted and
explicit zero context alike. It checks the context bound before configuration
dependencies. Saved settings carry one optional changed scope. Deserialization
checks its context bound and normalized run construction checks the presence
of a source. An explicit revision string is retained verbatim.

The Rust adapter compares both public CLI parsers, raw construction, saved
run settings, and plan restoration. It verifies the normalized scope or the
first diagnostic at each boundary. The sensitivity check detects dropping
explicit zero, checking the source before the context bound in raw settings,
and ignoring a supplied revision. The comparison ends before Git resolves
revisions or computes changed lines.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-config corpus/changed-config.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test cli_config changed_selection_configuration_matches_lean
```

## Line selection coverage

The Lean model treats each selection as an inclusive interval. It proves
that a query is covered exactly when its start and every in-query line
immediately after an input interval are covered. This check uses the same
number of boundary probes for a narrow interval and one ending at
`u32::MAX`. A separate theorem shows that merging overlapping or adjacent
intervals preserves the selected lines.

The corpus checks all 256 ordered pairs of intervals with endpoints in
0 through 3 against all 16 queries in that domain. Twelve additional cases
cover long intervals, file isolation, unsorted input, duplicates, and files
with multiple target slices. An empty or uncovered slice disqualifies its
file even when another slice is covered. The corpus also checks the core
boundary where typed explicit selections can contain 0 and changed-line
normalization discards ranges starting at 0.

The Rust adapter calls `LineSelectionIndex` and `normalize_changed` with
canonical relative paths. It compares range coverage, membership of the
query start, normalized changed intervals, and the covered path set.
Sensitivity witnesses detect checking only endpoints, requiring a single
input interval to cover the query, and accepting a file with any one covered
slice. Path normalization and Git diff parsing are outside this comparison.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-line-coverage corpus/line-coverage.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test target_policy line_selection_coverage_matches_lean
```
