# Lean Exception and Match Binding Correspondence Audit

Date: 2026-08-12

## Result

PASS for the repaired audited surface. The Lean model's 25 fixed schema-1
cases are current, all 15 implementation-facing internal fixtures match the
Rust analyzer's test-only projections, and all 8 strict public cases match the
`hoimin plan` manifest. The public set contains 1 present and 7 absent
candidate expectations. No same-premise production mismatch was found, so no
production transfer rule or public analyzer API changed.

This result is fixed-case correspondence evidence. **Lean proves only the
reduced model described below; it does not prove the Rust or Python
implementation.** Rust tests separately establish agreement at the exercised
internal and public observation sites.

## Claim and audited surface

The durable claim is that Hoimin's known-typing-import facts obey these binding
rules at the audited Python sites:

- an exception handler's type is observed before its target is bound;
- the target is shadowed in the handler body and deleted from the actual
  categorized handler exit after fallthrough, `break`, `continue`, or the
  shared `terminate` category;
- the return and raise cleanup cases remain distinct Python sources even
  though both map to `terminate` in the reduced model and analyzer;
- handler joins meet the selected, cleaned path with non-selected paths while
  preserving an unrelated supported `Mapping` import;
- partial-pattern and false-guard writes reach the following case, where they
  are observed before any compensating `Sequence` reimport;
- a refutable unmatched path without the known `Sequence` fact suppresses the
  known fact reimported by the completed path, while dropping that unmatched
  path incorrectly retains the fact;
- an irrefutable case removes its unmatched path; and
- unrelated supported imports survive ordered match-case propagation.

Every cleanup fixture reimports `Sequence` inside its handler immediately
before its exit. The private adapter selects the containing handler by its
unique reached marker, executes the production collector, and reads exactly
one state from the corpus row's `expected_exit_category` immediately after the
existing cleanup loop. It does not reproduce cleanup logic. Category-specific
test mutations make fallthrough, break, continue, and terminate cleanup
omissions retain `Sequence`; the separate return and raise rows both detect
the terminate omission.

The corpus remains 25 fixed, human-readable rows: 13 `handler` and 12
`match-case` rows.

| Mode | Observation | Rows | Correspondence conclusion |
|---|---|---:|---|
| `internal-fixture` | 10 annotation facts and 5 post-cleanup exit facts | 15 | All matched owned `#[cfg(test)]` production-backed projections. |
| `model-only` | Exact `unknown` / `shadowed` resolution | 2 | Lean-only; deliberately not compared with Rust. |
| `strict` | Public `type_list_sequence` candidates | 8 | All matched `hoimin plan` on count, presence, operator, original, replacement, and symbol. |

The two model-only cases are `handler_nonselected_join` and
`match_partial_failure_next_case`. Rust's production `KnownImports` state does
not expose the model's exact `unknown` versus `shadowed` distinction. Separate
implementation-facing rows observe the same transitions through known-fact
presence without inventing provenance.

The public harness writes one source into an isolated fixture project and
launches Cargo's built `CARGO_BIN_EXE_hoimin` as a subprocess for `plan`, with
one analyzer job and a 10-second deadline. It parses `PlanManifest` and retains
all candidates whose byte span overlaps the unique marker. More than one
overlapping candidate is a semantic mismatch, not infrastructure failure.

Fixture create/write failures; spawn, wait, timeout, nonzero exit, signal,
core dump, malformed JSON/manifest, invalid span, stderr on success, and
missing or duplicate marker failures are `infrastructure-error` and make no
semantic claim.

## Formal result and model boundary

`ExceptionMatchBindingProofs.lean` contains 12 universally quantified
theorems over reduced `BindingFlow` environments and exits:

1. handler type-before-target and body-after-target ordering (2);
2. target cleanup for fallthrough, break, continue, and terminate (4);
3. unrelated-name preservation and handler meet behavior (2);
4. pattern-failure and false-guard propagation (2); and
5. irrefutable exhaustion and inclusion of refutable unmatched state (2).

Ten concrete decidable witnesses distinguish broken definitions: one
bind-before-type witness, four exit-category cleanup witnesses, and one each
for handler join, wrong pre-pattern state, wrong pre-guard state, discarded
refutable unmatched state, and retained irrefutable unmatched state.

The executable exposes seven stable sensitivity families, all detected:

| Sensitivity family | Deliberate defect detected |
|---|---|
| `bind-before-type` | Observe the handler type only after binding its target. |
| `handler-exit-cleanup` | Retain the target on fallthrough, break, continue, or terminate; return and raise separately exercise terminate. |
| `handler-join-meet` | Keep the selected path instead of meeting both paths. |
| `pattern-failure` | Keep the successor reachable but use the pre-pattern environment. |
| `guard-failure` | Keep the successor reachable but use the pre-guard environment. |
| `refutable-unmatched` | Drop the unmatched path, changing internal facts and the paired strict candidate expectation from absent to present. |
| `irrefutable-exhaustion` | Retain an unmatched path after an irrefutable case. |

The refutable-unmatched sensitivity is checked at the corpus boundary: correct
internal facts are empty and the paired strict candidate is absent, while the
literal broken projection retains `direct:Sequence=typing.Sequence` and would
make the candidate present.

This is not bounded exploration: `structured_depth=0` and
`generated_depth_expansion=false`. The result covers the fixed semantic
equivalence cases plus the quantified theorems in the reduced two-name fact
lattice. Parsing, marker selection, JSON, files, process execution, and actual
Ruff/Rust control flow are outside Lean and are covered only by executable
tests.

## Exclusions

The audit excludes arbitrary Python exception and pattern syntax, parser
correctness, `except*` unless it follows an already exercised implementation
path, runtime exception-object lifetime, exact return-versus-raise identity
beyond their separate sources over the shared terminate category, arbitrary
names outside the supported `Sequence`/`Mapping` fixtures, mutation ranking
and execution, concurrency, performance, and generated or exhaustive case
exploration.

The audit does not infer semantics from infrastructure failures, compare the
two model-only resolution labels with a different Rust observation, or claim
that passing public fixtures proves unexercised analyzer behavior.

## Mismatch and correction ledger

No confirmed production defect remains. The final review found four evidence
defects, all repaired without changing production transfer behavior:

- five handler rows read pre-statement state and were already shadowed; their
  fixtures now reimport `Sequence`, and a category-selecting test-only hook
  reads the real post-cleanup exit;
- pattern and guard rows reimported before observation, and their mutations
  removed reachability; observations now precede reimport and reachable broken
  transitions use the wrong pre-pattern or pre-guard environment;
- the refutable unmatched polarity normalized correct and broken results to
  the same empty facts; completed and unmatched premises are reversed so
  dropping unmatched now retains a known fact; and
- same-line marker fallback accepted semicolon-separated dead code; fallback
  now requires horizontal whitespace followed by a `#` comment introducer,
  while real trailing-comment markers remain supported.

Earlier authoring corrections remain classified as model, fixture, or adapter
work rather than production defects: exact Lean labels stayed model-only,
unsupported `typing.TypeAlias` fixtures became supported `Mapping` fixtures,
and marker selection was narrowed around unreachable and compound nodes.

## Corpus ownership and freshness

The JSONL is rendered only by
`ExceptionMatchBindingAuditMain.lean -- --output`; it was not hand-edited. The
final guarded temporary output and freshness check both matched the committed
corpus byte-for-byte.

- SHA-256:
  `8c4feb9e30742cbf9b74a6bd0f9bc7cd7c90dcb0751d03a0b67796eec76fea63`;
- 25 unique IDs and 25 markers occurring exactly once in their source;
- modes `15 internal-fixture / 2 model-only / 8 strict`;
- families `13 handler / 12 match-case`; and
- kinds `10 annotation / 5 exits / 2 resolution / 8 public-candidate`.

## Guarded Lean resource ledger

Every final Lean command ran alone through
`tools/lean_resource_guard.py` with a 20-second deadline, a 768 MiB (786,432
KiB) root-plus-descendant RSS cap, and 250 ms sampling. Lake builds used
`-Kjobs=1`; executable cases used `lake env lean --run`. No aggregate library
or native-link retry was attempted.

| Final check / inner command | Exit / reason | Elapsed ms | Peak RSS KiB |
|---|---|---:|---:|
| Proof build: `lake -Kjobs=1 build HoiminOracle.ExceptionMatchBindingProofs` | 0 / `child_exit` | 555 | 56,368 |
| Cases build: `lake -Kjobs=1 build HoiminOracle.ExceptionMatchBindingCases` | 0 / `child_exit` | 285 | 2,784 |
| Proof consumer: `lake env lean /tmp/hoimin-exception-match-proof-consumer.lean` | 0 / `child_exit` | 2,697 | 645,536 |
| Cases: `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --cases` | 0 / `child_exit` (25/25) | 556 | 613,552 |
| Sensitivity: `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --sensitivity` | 0 / `child_exit` (7/7) | 555 | 661,072 |
| Stats: `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --stats` | 0 / `child_exit` | 549 | 681,744 |
| Temporary output: `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --output /tmp/hoimin-exception-match-final-corpus.jsonl` | 0 / `child_exit` | 551 | 679,984 |
| Freshness: `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --check corpus/exception-match-binding-correspondence.jsonl` | 0 / `child_exit` | 553 | 680,112 |

The highest sampled RSS was 681,744 KiB, 104,688 KiB below the fixed cap.
`cmp` returned 0, and a final process listing found no Lean/Lake executable.
No final command timed out, reached the RSS limit, or returned a monitor error.

The first fix-wave guard invocation inside the filesystem sandbox returned
exit 126 / `monitor_error` after 34 ms with no sample because process-tree
inspection was blocked. The same command was rerun once with unchanged limits
and monitoring permission and passed; the setup failure is not semantic or
resource evidence.

The superseded Task 1 first-GREEN parse failure is intentionally non-retained
evidence here. `task-1-report.md` records its layout syntax error, 662,576 KiB
peak, syntax-only correction, and successful rerun. It was authoring history,
not final semantic or resource evidence. The optional aggregate `HoiminOracle`
build that earlier exceeded the cap was not retried.

## Executable verification

Focused private verification passed all 8 exception/match tests, including the
five post-cleanup rows, reachable wrong-environment mutations, refutable
unmatched omission, unreachable markers, and the semicolon-dead-code
regression. Public verification passed 8/8 exception/match tests. Prior audit
integrations remained green: 4/4 annotation-scope public tests, 4/4
binding-flow public tests, and the focused control-flow and match-propagation
unit regressions.

The final workspace command `cargo test --workspace --all-features -j 2`
exited 0 for every target, with only declared ignored tests. The worktree-only
`.venv -> ../../.venv` setup symlink required by an existing ranking-oracle
test was removed immediately after the run. Workspace Clippy with
`-D warnings`, formatting, and diff checks also exited 0.

Exact final commands:

```bash
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --lib exception_match_binding -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_exception_match_binding_oracle
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_annotation_scope_oracle
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_binding_flow_oracle
cargo test --workspace --all-features -j 2
cargo clippy --workspace --all-targets --all-features -j 2 -- -D warnings
cargo fmt --all -- --check
git diff --check main...HEAD
git status --short
```
