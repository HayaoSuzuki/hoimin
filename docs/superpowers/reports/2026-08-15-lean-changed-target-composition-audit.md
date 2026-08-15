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

The closed corpus contains ten rows. Each row is rendered from a typed Lean
`AuditCase`; `eligible_lines` is computed by enumerating `CombinedEligible`, not
stored as an expected literal. Rust looks up each strict row by ID and compares
the public observation with those generated lines.

| Mode | Rows | Observation |
| --- | ---: | --- |
| `strict` | 7 | real Git fixture and public plan candidates |
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
The generated bounded range-set enumeration evaluates 1,365 membership states.
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
| proof module direct compile | 2,693 | 669,136 | 0 / `child_exit` |
| external proof consumer | 557 | 2,912 | 0 / `child_exit` |
| ten sensitivity families | 290 | 336 | 0 / `child_exit` |
| eight fixed cases | 290 | 2,608 | 0 / `child_exit` |
| corpus freshness | 293 | 2,464 | 0 / `child_exit` |

No retained command reached either limit. Peak RSS remained 117,296 KiB below
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
