# Fault-localization evaluation review

## Implementation self-review

1. Candidate identity: preflight rejected a SHA256/BLAKE3 mismatch. Read the core
   implementation and changed the experiment and its byte-span regression tests
   to BLAKE3. Existing source and span mismatches still fail before writes.
2. Observability: ephemeral port values made raw assertion rendering unstable on
   repeated original runs. Separate outcome and fingerprint stability, preserve
   unavailable fingerprint rankings, and avoid arbitrary message normalization.
3. Lifecycle: independent review found restoration after unconfirmed cleanup.
   Add a fatal containment exception, quarantine the copy and abort further cases.
   Tests reproduce the old restoration and confirm it cannot occur afterward.
4. Completeness: independent review found a final budget-exhausted repeat could
   satisfy the repeat-count check. Test and implement explicit budget termination
   status, retaining partial evidence without claiming sample completion.
5. Inputs and isolation: validate prepared source/test hashes and input inventory,
   create an owned copy, and use a fresh bytecode-cache prefix per observation.
   A real timestamp-cache fixture demonstrated the stale-bytecode bug before fix.

## Test self-review

1. Independent oracles: literal Ochiai examples and tie intervals, plus Hypothesis
   permutation, bounds, monotonicity and Unicode-byte-span properties.
2. Observation boundaries: real subprocess cases for normal outcomes, coverage,
   skips, xfail/xpass, collection failure, fixture failure and teardown failure.
3. Resource boundaries: child-process timeout, exhausted pre-launch budget, final
   repeat exhaustion, output limit and fatal cleanup/quarantine scenarios.
4. State boundaries: stale candidate bytes, exception restoration, immutable
   caller input, prepared-test modification and same-size timestamp bytecode.
5. Empirical validity: require fixed-version success and repeated buggy failures;
   keep unstable observations, rejected cases and partial sampling visible.
   Existing upstream regression tests remain unchanged; newly written tests use
   one case per invocation, parametrization/Hypothesis and explicit AAA sections.

## Independent review

A separate reviewer checked the design and implementation without changing files.
Design findings covered aggregate deadlines, omission targets and pytest special
outcomes. Implementation findings covered budget completion, cleanup, cache reads
and provenance/isolation. Significant findings have dedicated regression tests.
A final evidence review added RED-to-GREEN checks for incorrect eligibility
counts, exhaustiveness claims, and missing harness identities. These changes
affect evidence checking only; the frozen experiment scripts remain unchanged.
The full verification results and final design/documentation scores are recorded
with the completed evaluation report.

## Final design assessment

The software-design-philosophy review scores this bounded research harness
10/10: all eight diagnostics pass after the review fixes. This assesses module
complexity, not localization accuracy or readiness as a supported product API.

| Diagnostic | Evidence |
| --- | --- |
| One-sentence module purposes | The design assigns observation, inference, execution, preparation, and evidence checking to named owners |
| Interfaces simpler than implementations | `observe`, `command`, and `evaluate_case` hide pytest hooks, Windows jobs, and copy lifecycle |
| Implementations change independently | Score formulas don't consume process objects; containment doesn't depend on scoring |
| Interface comments describe abstractions | Comments identify comparability, frozen selection, quarantine, and completion contracts |
| Design belongs in review | Five design passes and independent review changed lifecycle and observation decisions |
| Modules hide important decisions | Scoring owns tie semantics; collection owns test phases; execution owns process lifetime |
| Boundaries understandable without implementation | The design documents the normalized observation fields and owners |
| Design improvement receives explicit effort | Dedicated passes replaced the combined driver, fixed cache isolation, and clarified partial completion |

No diagnostic remains failed in this scope. A product implementation would need
another assessment for protocol versioning, backend guarantees, plugin support,
and general source mapping. The evaluation doesn't measure the percentage of
development time spent on design. Lean wasn't used: the decisive risks concern
observed pytest behavior and operating-system process lifetime. Formula and
ranking properties have independent examples and Hypothesis checks; they aren't
presented as formal proofs of the collector or runner.

## Documentation audit

**Score:** 10/10 — ships as is. **Shippable:** yes.
**Diagnostic:** 10/10 — failed rows: none after the edits listed here.
**Doc type / reader:** Research report and reproduction guide for maintainers.
**Language:** English.
**Local style guide:** `docs/development/guides/documentation.md` and
`docs/development/guides/okf-workflow.md`; research stays outside user guidance.
**Blocking:** None after checking the final evidence, commands, and local links.

The technical-documentation review resolved these findings:

| Location | Rule | Before | After | Severity |
| --- | --- | --- | --- | --- |
| Report result interpretation | P11, verified behavior | A rank improvement could imply saved investigation time | Separate measured ranks from unmeasured human effort | Blocking |
| Reproduction prerequisites | R3, prerequisites before steps | Local server permission and dependency assumptions were implicit | State Python/tool requirements and localhost use before commands | High |
| Reproduction verification | R5, verification and next step | Driver completion could be mistaken for successful evidence | Check records, explain partial status, and link remaining research | Medium |
| Provenance | P11, verified behavior | Byte hashes could reject a Git-normalized checkout | Explain LF/CRLF-only equivalence in the checker | Blocking |

**Rewrite plan:** Completed: scope and result interpretation, runnable procedure,
verification and links, then wording. The report preserves negative results and
unmet adoption conditions.

## Verification record

Checks run on Windows for this change:

- Research harness: **61 passed** in 19.43 seconds, including seven altered-evidence
  rejection cases and the process/source boundary tests.
- Repository Python suite: **2,684 passed, 76 skipped** in 303.23 seconds.
  `GIT_ALLOW_PROTOCOL=file` confines release-fixture Git operations to their
  temporary local bare remotes. The initial restricted run encountered Windows
  sandbox process errors; all 79 affected cases passed after that environment
  constraint was resolved, before the complete successful run.
- Ruff isolated `F,I,E9` checks and formatting checks: passed for this directory.
- Strict documentation build: `python -m tools.docs_site --output site`, exit 0.
- OKF YAML/reserved-file structure: 65 Markdown files passed. Reference inventory:
  zero missing Markdown targets, zero missing source targets, zero unreachable
  concept pages.

The final evidence check recomputes both public cases successfully. All 1,109
generated candidate source hashes and byte spans match the pinned public source.
The evidence checker and final public-case results are recorded in the
[summary](evidence/summary.json). The repository suite doesn't discover the
research directory by default; run its explicit command in the README when
changing this harness. Production Rust source is unchanged; the CI follow-up
below changes one existing integration-test helper.

## CI follow-up: isolate name-binding test processes

The [randomized-order job](https://github.com/HayaoSuzuki/hoimin/actions/runs/44992028342/job/137115463832)
failed before running the mutation assertion: the shared temporary workspace's
coordinator lock exhausted its five-second bootstrap deadline. Both real-run
tests used the ambient managed-root namespace. Other integration fixtures already
isolate that namespace, including `tests/disk_shutdown.rs`.

A controlled Windows reproduction initializes a separate temporary managed root,
holds its coordinator's byte lock, and runs
`first_iterable_mutation_is_detected_by_real_run` with that ambient temporary root.
Before the fix, the existing test fails with the same bootstrap deadline error
after 5.02 seconds. After the fix, it passes under the same held lock in 0.52
seconds. This demonstrates independence from a busy unrelated namespace; it
doesn't identify which process held the lock in the hosted failure.

The helper in `comprehension_named_bindings.rs` gives each child its own temporary
root through `TMPDIR`, `TMP`, and `TEMP`. Review checked five boundaries: reuse of
the established isolation pattern, child-only environment changes, temporary-root
lifetime through child exit, separation from the source fixture, and preservation
of production lock/deadline behavior. It doesn't retry or increase a timeout.

Validation: all **82** cases in that integration target pass on Windows with eight
test threads; `cargo fmt --all -- --check` passes. The CI run linked here is the
failure evidence, not the post-fix result. Use the PR's latest checks for hosted
verification of the fix.
