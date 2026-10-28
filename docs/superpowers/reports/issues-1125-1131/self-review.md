# Self-review record

These are separate passes by the implementing agent, not five independent
reviewers. Commands and actual outcomes below distinguish prototype evidence
from product implementation. Research base: main `42130ce3` (2026-10-28).

## Stage 1: research (five passes)

1. Scope pass: read all seven current Issue bodies; retained evaluation gates,
   arbitrary test commands, static discovery and requirement-based interpretation.
2. Specification pass: checked every original weak/strong pair and explicit
   control domain. The explicit-argument callback control produces TypeError;
   it is not counted as a missing-test success. Renamed the factory control to
   read-only, because its lists are mutable despite the narrower contract.
3. Comparison pass: inspected candidate descriptors/order. Restricted the direct
   continue comparator to `break_continue`, retaining standalone-call deletion
   as a documented related operator; equal budget means an upper bound, not
   fabricated work when a pool is empty. Full-default controls are separate.
4. Cost pass: bounded execution, disk reserve and cleanup checked. Added explicit
   generation wall times; avoided equating AST prototype time with production
   CLI planning. No memory or real-project speed improvement claim.
5. Reproducibility pass: ran the fixed corpus on CPython 3.14.9 and 3.15.0b1
   (beta), inspected JSON verdicts, preserved actual binary hashes/revisions and
   original controls. Candidate generation remains pure AST; only tests import
   the subject. The 14 examples are illustrative, not a broad efficacy trial.

## Stage 2: static producers (five passes)

1. Identity pass: added typed identities without reordering existing variants;
   reused existing name-resolution flow, including wildcard and dynamic writes,
   to recognize builtin NotImplemented. No new import resolver/dependency.
2. Scope pass: tested non-methods, arithmetic methods, nested functions,
   decorated/async functions and generators. A generator negative case failed
   with one unwanted comparison candidate; reused plain_synchronous and observed
   green. Return eligibility is restored across nested function definitions.
3. Syntax pass: reparsed all selected edits; exercised parenthesized causes,
   comments, UTF-8 offsets and nested try/with/finally/loop-else. Cause replacement
   edits the Name only; existing surrounding trivia remains.
4. Resource pass: checked inherited payload/admission path, truncation, line and
   symbol selection, and immediate cancellation. No extra candidate collection
   or persistent target state was introduced.
5. Behavioral pass: three new default-candidate assertions failed before product
   changes, then passed. Added negative coverage for absent/None/attribute/call/
   conditional causes. The product intentionally has narrower eligibility than
   the research prototypes.

## Stage 3: integration (five passes)

1. Catalog pass: independently updated enum-name and current-default fixtures,
   exhaustive ranking categories and valid-Python producer inventory. Catalog is
   97 IDs/78 defaults; new producers are honestly deferred from the initial Lean
   corpus rather than silently presented as formally modeled.
2. Defaults pass: public plan test includes all three additions and individually
   excludes each while comparing all remaining descriptors/identities. Existing
   explicit-selection tests still pass; no family membership was expanded.
3. CLI pass: actual plan/verify tests confirm weak survival and stronger kills,
   baseline success, complete counts, zero errors and unchanged sources. Added
   separate CPython output contracts for context cleanup/loop else, reflected
   comparison and explicit cause versus implicit context.
4. Documentation pass: updated current usage/counts/scopes and generated CLI
   reference. Dated the old slice notice rather than implying its historical
   75/94 counts are current. Preserved historical audits and their metadata.
5. Attribution pass: added one draft OKF decision and catalogued all three new
   report Markdown files, with actual base revision and source hashes. The
   reference checker sees 62 pages; 62 Python docs/boundary tests pass. Final
   source-hash validation follows the final review-record update.

## Independent review

A fresh read-only reviewer inspected the final scope enum/helper, source and
new tests/docs without builds or edits. It reported no actionable findings.
It independently checked both JSON files against all 14 report rows, original
passes, two-candidate ceilings and cleanup. Pending final validation/hashes
were explicitly outside its verdict.

## Stage 4: final verification

Clippy initially found a fourth boolean field, a visitor exceeding the line
limit by one, and one missing diagnostic message on an empty-list assertion.
Replaced comparison eligibility with ComparisonReturnScope and extracted its
function-scope decision; fixed the assertion and renamed the shared tracked
builtin list to CONTROL_BUILTINS. These retain behavior without lint suppression.
The final commands/results and five review passes follow below.

### Final five passes and observed results

1. Regression pass: `cargo test --locked --workspace --no-fail-fast` completed
   with 194 suite summaries, 3,969 passed, 0 failed and 34 ignored. The full run
   needed local Unix-socket permission for an existing test; no assertion was
   weakened. The final typed-scope form additionally passed all five focused
   analyzer tests. Workspace all-target/all-feature Clippy passes with warnings
   denied; workspace and vendored parser format checks pass.
2. Evidence pass: fixed research runs on CPython 3.14.9 and 3.15.0b1 agree.
   Per interpreter, the fixed budget executes 12 direct, 22 full-default and 16
   proposed candidates; all 28 original weak/strong checks pass. Python 3.15 beta
   also passes four default/opt-out/plan-verify tests and three dedicated product
   behavior contracts. This is beta evidence, not stable-3.15 certification.
3. Review pass: no actionable findings from the independent reviewer. Rechecked
   the scope-enum extraction against the boundary suite and Clippy; no lint
   suppression was introduced. Seven generated-reference tests and its `--check`
   pass. Sixty-two Python docs/boundary tests pass.
4. Artifact pass: the OKF reference checker reports 62 pages with no missing
   Markdown, source or unreachable pages. Safe YAML parsing validates 58 concept
   headers plus reserved-file structure; 12 new-batch source hashes are checked
   after this record is finalized. `git diff --check` passes. Source edits stay
   within operator generation/integration; no Cargo dependency/lock change,
   legacy reader, runtime state manager or private-directory change is included.
5. Resource pass: builds used two jobs, no incremental compilation/debug symbols,
   and two Rust test threads. Temporary probe directories were removed on both
   Python runs; the one local py_compile cache was removed. Cargo target reached
   about 5.1 GiB with ample free space. Keep it for commit hooks, then run
   `cargo clean` after they pass; the PR description records the actual final
   cleanup result. No peak-memory measurement or hard macOS containment is claimed.

Remaining evidence limits are real-project efficacy and unexecuted platforms;
no code-review finding or failing required check remains. Existing ignored
manual benchmarks are not reported as executed or as new performance evidence.

## Follow-up: maintainer-directed reconsideration

The maintainer requested proceeding first with 1125/1129/1130 and reconsidering
methods for 1126/1127/1128/1131. This supersedes the initial four deferral/closure
conclusions. The three product implementations and their recorded validation
are unchanged. No extra Rust operators are introduced by this follow-up.

### Method review (five passes)

1. Evidence classification: separated observed useful leads from implementation
   uncertainty. Factory sharing and encoding loss both have positive leads;
   existing-resolver limitations do not demonstrate low usefulness.
2. Consistency: one sorting/deletion overlap is not universal redundancy.
   Receiver type proof is not imposed uniquely on encoding methods while other
   nominal method operators exist. Proposed trusted-codecs scope also handles
   parameter values without type inference.
3. Call interface: replaced the parameter-removal proposal with a private
   getter/sentinel wrapper. Checked same-value positional overrides, keyword
   None and mutable identity, with helper-name collision cases. Documented
   allocation/default metadata costs instead of claiming exact signature identity.
4. Static boundaries: specified ordinary dataclass class/import evidence without
   relaxing shared resolve(); sorted builtin identity and limited key shape;
   loop/scope restrictions for captures; explicit encoding errors policy.
   These are design requirements, not claims of implemented name analysis.
5. Probe scope: syntax-checked both sides, used fresh bounded Python processes,
   required all original controls to pass and intended strong mutant assertions
   to fail. Nine cases on 3.14.9 and 3.15.0b1 beta pass those expectations. No
   new full incumbent comparison or real-project efficacy is claimed.

### Delivery review (five passes)

1. Scope: retain only the three already-tested product operators in PR 1134;
   no Rust/default/dependency change for the four reconsidered proposals.
2. Completion: remove automatic closure of 1126/1127/1128/1131 and replace the
   report's all-seven-complete statement with the current maintainer direction.
3. Documentation: link revised methods and raw feasibility evidence; keep original
   measurement JSON and historical test counts unchanged.
4. Provenance: add the new report to the audit catalog and sources to the draft
   OKF concept; refresh hashes/revisions only for actually changed/new sources.
5. Verification: run fixed probes on both recorded interpreters, isolated Ruff,
   docs/boundary tests and OKF YAML/link/hash checks. No Cargo rebuild is needed
   for documentation/research-script changes; prior product test evidence is
   explicitly retained rather than reported as rerun.

Follow-up verification completed: both interpreters agree on all nine fixed
cases and their interface controls; isolated Ruff format/check pass. The 62
Python docs/boundary tests pass. OKF references cover 62 pages with no missing
or unreachable entries; 58 concept YAML headers and reserved files conform,
and all 17 batch source hashes match after this record update. No product
source or Cargo files changed in this follow-up.
