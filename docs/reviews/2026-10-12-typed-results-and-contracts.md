# Typed results and contracts review

The approved [design](../superpowers/specs/2026-10-12-typed-results-and-contracts-design.md)
and [plan](../superpowers/plans/2026-10-12-typed-results-and-contracts.md) each
record five separate self-review passes. Implementation uses the existing
`refactor/typed-results-and-contracts` branch. A final independent review follows
implementation and validation.

## Python implementation self-reviews

1. Checked failure typing: expected grammar and conversion failures return
   `Failure[ValueError]`; adapters raise the contained error. No broad catch or
   global deal state change was introduced.
2. Checked grammar: canonical stable versions and stable-or-dev validation remain
   distinct. Unicode digits, leading zeroes, trailing newlines, and conversion
   limits have behavioral coverage. Validation does not acquire an integer limit.
3. Checked compatibility: existing messages, `SEMVER`, numeric ordering and
   callers remain intact. The focused release/provenance suite passed 129 tests
   with 10 platform skips on Windows.
4. Checked dependency setup: internal-tooling is pinned and included in dev and
   provenance. All four pre-build entry points install it before use. A fresh
   Python 3.14.6 environment ran help and set-version against copied manifests;
   provenance-only imports also passed. No tag or publication operation ran.
5. Checked strict typing and contracts: the incorrect-tuple sensitivity test
   raises `deal.PostContractError`. returns constructors expose `Any` on their
   unused generic side; a minimal reproduction ruled out annotation-only fixes.
   Narrow casts at these immutable construction boundaries preserve concrete
   outward types without suppressing ty. Repository-wide Ruff, format and ty
   checks passed, as did 172 workflow tests. Frozen uv audits found no known
   vulnerabilities/adverse statuses on Linux, macOS and Windows (78 packages).

New Python tests first failed because the interfaces were missing (5 failures,
3 passes); all eight passed after implementation. Bootstrap regressions first
failed in all four jobs and then passed. Installed returns 0.29.0 uses BSD-3-Clause
and deal 4.24.6 uses MIT. Neither was added as a product runtime dependency.

## Execution decisions

- Reused the existing clean work branch in this session rather than making a
  second checkout. Concurrent manual branch edits would require reconciliation.
- Used narrow constructor casts after reproducing the upstream generic typing
  limitation. A future returns typing change may make these casts unnecessary.



## Rust implementation self-reviews

1. Reviewed contracts 0.6.11's published source and verified its archive checksum.
   `old` binds before the body; explicit returns become breaks from a labeled
   block, with assertions after the block. Conditions panic through assert even
   in release builds. The macro does not rewrite `?`. Reserve keeps its explicit
   error return with one explained, narrow Clippy exclusion; interior uniqueness
   assertions remain unconditional.
2. Reviewed feature behavior and cost: only cfg_attr activates the optional
   dependency. Default normal core dependencies exclude contracts and prohibited
   I/O crates. Snapshots hold a single u64, never collections. Counter tests pass
   enabled and disabled; optimized disabled tests pass 3/3 without evaluating
   snapshots or conditions. No global disable_contracts feature is selected.
3. Reviewed exit coverage and diagnostics: budget limit, allocator error, normal
   success, unknown release and repeat release all reach postconditions. Domain
   predicates remain ordinary methods. The new panic contains stable IDs,
   function and condition; the old budget Debug context payload is intentionally
   replaced. Other custom diagnostics keep their payloads (mapping below).
4. Reviewed dependency policy: only contracts =0.6.11 gains MPL-2.0 permission.
   Both locked graphs pass cargo-deny 0.20.2 licenses/sources with warnings denied
   and cargo-audit 0.22.2 (210 root / 93 fuzz packages). The proc macro has no
   custom-build target; the build-script inventory has no added/removed scripts.
   The root graph adds one package using existing syn 3.0.5/quote/proc-macro2.
   Fuzz does not enable the transitive optional feature, so its lockfile remains
   unchanged after regeneration. No gratuitous dependency upgrades were retained.
5. Reviewed third-party adoption versus alternatives: the attribute centralizes
   all explicit exits and supplies old-value accounting which manual exit macros
   lacked. Creusot requires a separate proof toolchain/specification effort and
   would not replace this runtime check. The dependency is unmodified; its MPL
   notice and source link are retained. This is bounded runtime checking, not
   a formal proof, and no new proof claim is made.

The new attribute counter test first failed (0 evaluations instead of 2), then
passed with the adopted attribute. The corrupt-ledger limit/release tests also
passed under the old implementation, confirming preservation rather than a new
coverage claim. A dedicated allocator-error regression guards the explicit return.

## Remaining custom contract sites

| Site / ID | Reason to preserve this check during the budget migration |
| --- | --- |
| machine.rs / machine.effect.once | Validates the pending map immediately after registration and includes pending/completion frontier context. Moving to broader transition exits changes timing and error coverage. |
| machine.rs / machine.worker.candidate.pre | Checks immediately before assigning a particular candidate, after fallible worker lookup. A function precondition would run before the worker/candidate branch is selected. |
| machine.rs / machine.transition.post | Checks successful registration against locally produced effects, with pending/phase/effect context; the transition uses multiple `?` exits which the adopted macro does not intercept. |
| machine.rs / machine.worker.invariant | Successful transition checkpoint with copy-grant, workers and pending diagnostics; blanket annotations would lose these local values and alter error-path coverage. |
| machine.rs / machine.budget.invariant | Same transition checkpoint includes process limits and pending execution context; retain the existing failure payload and scope. |
| machine.rs / machine.cancel.post | Compares entry fatal state and produced effects after registration; local was_fatal/effects diagnostic context should not be discarded in an unrelated migration. |
| report.rs / report.sequence.invariant | Producer assertion runs after validation and before any mutation. End-of-function checks would run too late and miss `?` validation errors. |
| target.rs / target.resolve.post | Success-only normalized target checkpoint retains actual target slices in panic context. A simple ensures would drop this payload and needs deliberate error-path mapping first. |
| target.rs / target.changed.post (normalize_changed) | Pure output check could be annotated, but the custom diagnostic includes the resulting ranges; retain that failure payload until an attribute context approach is designed. |
| target.rs / target.changed.post (intersect_changed) | Same normalization invariant with actual resulting target slices in context. Preserve the existing diagnostic rather than broaden this first component migration. |

Custom macros remain for these identified uses; they are not preferred over the
third-party crate for equivalent boundary checks. Their existing disabled
condition/context tests remain part of validation.

## Sensitivity and initial full-suite evidence

Two temporary Rust source faults were executed and restored before final checks:
reducing the inserted reservation amount caused budget.reserve.accounting to
panic; replacing the explicit allocator error return with `?` made the corrupt
allocator regression fail because its expected panic was bypassed. The restored
feature-enabled core suite and strict Clippy both passed. No fault is committed.

The initial full Python run passed 993 tests with 21 skips and one 120-second
shell-harness timeout in test_bounded_audit_covers_every_module_and_generator.
The isolated retry passed in 77.18 seconds. A final run after incorporating
merged documentation PR #800 will run without concurrent heavy compilation.

Free disk before builds was approximately 183 GB. The audit tool build target
was cleaned with cargo clean after installation (818.4 MiB removed). Shared
repository artifacts remain until CLI verification has finished.

Rust validation completed: default core 372 passed / 2 ignored; final contract-enabled core 360 passed / 2 ignored; contract-enabled CLI 2,420 passed / 25 ignored across 123 targets. Optimized disabled checks passed 3/3. Strict feature Clippy passed after the sensitivity faults were restored.

## Testing self-reviews

1. **Red/green and sensitivity:** Checked actual missing-interface failures,
   four bootstrap regressions, the deal incorrect-result rejection, attribute
   evaluation counts, and two restored Rust source faults. These detect behavior,
   not decorator presence. A single hoimin identity mutation (`parts is None`
   to `parts is not None`) was conclusively killed in the final bounded run.
2. **Malformed input and normal failures:** Reviewed grammar classes, Unicode,
   integer-conversion limits, stable/dev distinction, unchanged exception messages
   and ordering properties. Fresh ordinary invalid inputs do not become deal
   violations; the sensitivity test deliberately introduces a programming defect.
3. **Bootstrap and platforms:** Fresh Python 3.14.6 minimal-group help/set-version
   and provenance-only imports passed. All four early release entry points have
   installation-order regressions; Linux/macOS/Windows uv audits pass. Actual
   local execution is Windows; workflow inspection does not claim macOS/Linux
   execution or a real release publication.
4. **Rust feature and exit behavior:** Default core 372 passed / 2 ignored;
   feature core 360 passed / 2 ignored; feature CLI 2,420 passed / 25 ignored.
   Optimized disabled evaluation passed 3/3, corrupt limit/allocator/release
   paths are checked, and strict feature Clippy passes. Scalar snapshots are
   bounded; all retained custom checks have timing/context rationale above.
5. **Completeness and limits:** After merging main PR #800, the full Python suite
   passed 984 tests / 21 skipped in 294.08 seconds, with no failures. Repository
   Ruff check/format and ty, strict Material docs build, workflow lint and frozen
   three-platform uv audits (84 packages) pass. Both Rust license/source and
   advisory audits pass. This is local evidence; hosted checks are recorded
   separately after PR creation. Runtime contracts, properties and oracle tests
   are not a new formal proof.

## Mutation verification scope

Whole-tools planning encountered an analyzer panic, `duplicate unary-not operator
start`, outside the migrated release parser's isolated source. No whole-tools
mutation coverage is claimed. An exact-byte projection of the changed release
module, its eight tests, pyproject.toml and uv.lock passed the normal baseline.
The initial one-candidate verification exceeded the automatic five-second limit.
After diagnosis it was replanned with an explicit 30-second mutant timeout,
retaining one worker, 8 GiB owned-space ceiling and 10 GiB free-space reserve.
The same candidate completed: 1 killed, 0 survivors, no inconclusive outcomes.
Peak owned space was 528,131 bytes; cleanup reported no retained roots, and the
exact temporary project/report directory was removed on exit. The broader
analyzer panic was not investigated as part of this bounded library migration.

## Distribution and cleanup

Maturin generated ordinary wheel metadata with version 0.4.0, unchanged Python
requirement and no Requires-Dist entries. The normal dependency graph excludes
contracts and I/O dependencies in core. This maintenance/optional-check migration
keeps the existing 0.4 release floor; no artifact was published or tag reserved.

After verification, cargo clean removed 4.3 GiB of core outputs and 27.8 GiB of
CLI outputs; shared third-party artifacts were retained. Audit-tool build cleanup
previously removed 818.4 MiB. Final free disk after these operations was
187,597,234,176 bytes. No active build was cleaned.

Independent whole-branch review and hosted-check status follow below.

## Independent whole-branch review

A fresh GPT-6-astra reviewer inspected df40d8c..3f4354414505785f9171c99d2dbd8295b8a9fbb3
against the approved design/plan and actual upstream generation code. It reported
no Critical, Important or Minor findings and requested no code changes.
An independent read-only Python 3.14.6 probe compared 2,048 adapter outcomes with
base, including Unicode, leading zeroes, trailing newlines, dev suffixes and
oversized integers: all matched. git diff --check passed. The reviewer inspected
existing Rust/full-suite evidence instead of rerunning expensive builds.

Its assessment was ready to merge subject to hosted checks. Strengths included
fresh bootstrap coverage, preserved grammar/exception adapters, explicit Rust
error returns, disabled attribute/snapshot evaluation, retained interior checks,
scalar storage and version-scoped dependency policy/notices.

Declined to judge: diagnosis/repair of the broader analyzer's duplicate-unary-not
panic is outside this migration, and no analyzer implementation changes appear
in the diff. It limits whole-tools evidence without invalidating the byte-identical
focused projection or its conclusive one-candidate result.

Maintainer ruling: keep that analyzer investigation outside this library migration
and explicitly retain the limitation. If wrong, broader tools mutation planning
can remain unusable until separately diagnosed; this PR does not promise that
coverage. There are no deferred minor review findings.

## PR delivery and hosted snapshot

[PR #801](https://github.com/HayaoSuzuki/hoimin/pull/801) was created against
current main and attached to the task. At reviewed implementation head 78f41c0,
hosted documentation, workflow lint, both Rust license/source checks, both Rust
advisory audits and all three Python dependency audits passed. CI and release
validation were still running; no failure was observed at that snapshot. This
record is not a claim that subsequent heads or all hosted jobs have passed.

After the full merged-main suite, the final documentation/contract smoke suite
passed 23 tests in 2.77 seconds; the strict site build passed again after review
record changes. Production code did not change after the independent review.
The fresh bootstrap environment was removed after its validation. All changes,
including design, plan and review documents, are committed on the work branch.
