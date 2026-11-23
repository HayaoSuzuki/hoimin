# Standard-library contract mutations implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Execute each task with its review gate before the next implementation task.

**Goal:** Implement the adopted standard-library contracts, default enabled with individual opt-out, and deliver a reviewed PR with successful final-head CI.

**Architecture:** Private typed producers reuse trusted imports, processing-call source context and deferred candidate admission. Domain modules own API signatures and mutation semantics; the collector only routes calls and retains existing budgets.

**Tech Stack:** Rust, Ruff AST, rstest/proptest, CPython 3.14 and 3.15, pytest and the existing docs generator.

**Spec:** [Standard-library contract design](../designs/2026-11-23-stdlib-contract-mutations.md).

## Global Constraints

- All new operators are default enabled and individually excludable.
- Python 3.14 and 3.15, including Windows, remain supported.
- No new dependency, persisted schema, network service or user-facing CI gate.
- Public evidence uses synthetic examples only; no private-project information.
- Typed enum/match dispatch follows identifier recognition; no string-based behavior routing across modules.
- Each original expression remains in its lexical scope and evaluates once in source order, except the documented removed operation.
- Preserve native Boolean conversion timing and repeated conversions for dynamic options.
- Cancellation, selection, profile, delimiter depth and candidate-ranking checks precede replacement materialization; the existing sink charges retained payload after rendering. One temporary replacement remains outside that budget.
- Every test invocation exercises one scenario with Arrange/Act/Assert; use named parameterized cases and meaningful property tests.
- At least five self-review passes at design, plan, implementation and test stages, recording findings/corrections.
- Root owns the Cargo queue. Use CARGO_INCREMENTAL=0, CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0 and -j 2. Workers request test runs; never launch Cargo concurrently.
- Clean completed package outputs between major checks and all target artifacts after validation. No work-record ZIP.

## Review Focus

1. Effectful receivers, keyword expressions, walrus and zero-argument super must not evaluate twice or cross lexical scopes (tasks 1–3 runtime witnesses).
2. Dynamic policy objects can change or raise on later truth checks; helper frames must not introduce avoidable retention after errors (task 2 witnesses).
3. Module aliases that escape/rebind, local shadowing and lazy imports must not acquire false callable identity (each task discovery negatives).
4. Multiline/CRLF arguments, trailing comments and explicit-line selection must select the edited option and produce parseable source (each task properties and source tests).
5. Default-enabled additions alter unrelated default candidate inventories; test explicit contracts with explicit operators, and genuinely default-dependent fixtures with the new behavior (task 4 integration).

## Task 1: Text, Unicode and encoding contracts

**Files:** Create `crates/hoimin-python/src/analyzer/rust/stdlib_contracts.rs` and `stdlib_contracts/text.rs`; create `crates/hoimin-python/tests/stdlib_text_mutations.rs` and `crates/hoimin-cli/tests/stdlib_text_witnesses.rs`. Modify core `config.rs`, CLI `plan/ranking.rs`, analyzer `rust.rs`, encoding recognition in `followup_calls.rs`, and `docs/user/reference/operators.md`.

**Interfaces:** Produce `stdlib_contracts::Imports::build(module: &ModModule, operators: &MutationOperatorSelection, cancelled: &impl Fn() -> bool) -> Result<Self, AnalysisCancelled>` and `Imports::emit(&self, call: &ExprCall, site: &processing_calls::Site<'_>, emit: impl FnMut(TextRange, TextSize, MutationOperator, &dyn Fn() -> String) -> bool)`. The callback separates the edit range from the selection offset, refined during task 2 review. Domain producers own eligibility; root collector supplies a deferred sink. Module-private helpers may evolve in later tasks but do not expose domain identifiers as strings.

- [x] Write public discovery/default/opt-out tests using operator IDs as strings before registration. A named case for each of unicode_digit_predicate_swap, unicode_digit_ascii, unicode_normalization_form, unicode_normalization_remove, string_casefold_lower, string_affix_strip, encoding_errors_replace, encoding_errors_strict must discover a candidate by default and none when excluded. Existing encoding_errors_ignore must gain omitted/positional/dynamic-policy cases.
- [x] Root runs `cargo test --locked -j 2 -p hoimin-python --test stdlib_text_mutations`; capture intended missing-registration/behavior failures.
- [x] Implement and register the eight new IDs, default list and ranking. Use private enums for predicate, normalization form, affix and error policy. Consolidate encoding recognition without duplicated candidate production. Keep valid existing ignored-policy cases.
- [x] Add named runtime witnesses: fullwidth 2 rejected only by ASCII narrowing; superscript 2 distinguishes decimal/digit; fraction one-half distinguishes numeric; NFKC fullwidth 1 becomes ASCII; NFC/NFD composed accent differs; sharp-s lower versus casefold; `ababaX` prefix removal returns `abaX` versus strip `X`; invalid UTF-8 rejects/replaces/ignores; codec positional/keyword argument effects occur once in source order; custom nominal receiver behavior is explicitly tested/documented.
- [x] Add properties for ASCII digit predicate invariance, admitted replacement parseability and cap-prefix stability. Add alias/shadow/duplicate-binding negatives and line-selection tests; group test setup helpers only when they hide useful mechanics.
- [x] Record five implementation and five test self-reviews. Root runs focused analyzer + CLI witnesses, formatting and review diff. Update operator reference entries and commit the independently working task after task review.

## Task 2: Iterator and serialization option contracts

**Files:** Extend `stdlib_contracts.rs`; create `stdlib_contracts/options.rs`, `stdlib_contracts/urls.rs`, `crates/hoimin-python/tests/stdlib_option_mutations.rs`, `crates/hoimin-cli/tests/stdlib_option_witnesses.rs`. Reuse/refactor `dynamic_boundaries.rs` option rendering if the shared contract earns it. Update registry, ranking, collector and operator reference as needed.

**Interfaces:** Consume the task 1 Imports build/emit facade and processing Site. Domain-private typed API/signature descriptors encapsulate keyword roles. A shared Boolean-policy renderer owns omission insertion, explicit value replacement, grouping and native truth behavior; existing zip integration must retain its witnesses if changed.

- [x] Write discovery/default/opt-out tests for map_strict_toggle, batched_strict_toggle, url_quote_swap, url_query_policy, base64_validate_toggle and json_allow_nan_toggle. Root runs the target before implementation to record failures.
- [x] Implement the six IDs. Add omitted defaults; explicit Boolean literals remain handled by boolean_literal, with no duplicate candidate under the new ID. Validate signatures and default differences; JSON custom cls is excluded except None. Python 3.15 Base64 ignorechars must not falsely imply default validate=False.
- [x] Runtime cases: map `[1,2]` with `[10]` truncates versus raises; batched length 3 size 2 emits final short tuple versus raises; quote `a b/c` distinguishes `%20`/`+` and slash; unquote `a+b`; urlencode list values; parse_qs empty value; b64decode `YQ==!`; json NaN; dynamic policy truth count/order before and during iteration; skipped truth paths and retained exception objects; one source scenario per invocation.
- [x] Properties cover unchanged ASCII URL inputs without special characters, duplicate-option exclusion, candidate cap prefix and parsing of grouped generator/multiline calls. Validate shared zip behavior if refactored. Root runs focused suites and both interpreter witnesses.
- [x] Record five implementation/test review passes, complete task review, update operator reference and commit.

## Task 3: Numerical and heap contracts

**Files:** Extend facade; create `stdlib_contracts/numeric.rs`, `crates/hoimin-python/tests/stdlib_numeric_mutations.rs`, `crates/hoimin-cli/tests/stdlib_numeric_witnesses.rs`. Update registry, ranking and operator reference.

**Interfaces:** Consume the existing facade and trusted import/destination helpers. Typed numerical API descriptors own signature differences including statistics center keywords; effectful original operands remain outside synthetic bodies.

- [x] Write discovery/default/opt-out tests for math_isclose_tolerance, math_precision_loss, statistics_sample_population and heap_pushpop_replace. Run before implementation through root.
- [x] Implement the four IDs and their documented contracts. Preserve function/argument evaluation; avoid arbitrary math function exchanges. Skip known-zero tolerances and omit the already-zero abs_tol default.
- [x] Runtime witnesses: isclose(0,1e-10,abs_tol=1e-9) changes True to False; omitted rel_tol accepts a nearby nonzero pair but zero tolerance rejects; x=1e-16 exposes expm1/log1p precision loss; variance([1,2,3]) is 1 versus pvariance 2/3; keyword centers retain evaluation order; heap [5,9] and added 3 returns 3 versus 5 with corresponding heap state; empty-heap failure is intentional.
- [x] Properties cover finite zero-tolerance isclose equality, population/sample relationship for at least two finite values, and heap multiset conservation across insertion/removal. Add aliases, function references that escape, argument-shape negatives, caps and line selection. Root runs focused targets with both CPython versions.
- [x] Record five implementation/test review passes, complete task review, update reference and commit.

## Task 4: Integration, public evidence and delivery

**Files:** `docs/user/reference/cli.md`, `compatibility.md`, relevant core/CLI inventory fixtures, `docs/development/knowledge/design/analyzer.md`, knowledge index metadata as required, and this design/plan.

- [x] Generate the CLI reference from the existing shared document source; validate IDs/default/exclusion/help/plan-verify discovery. Expected catalog is 173 and default runtime count is 154, assuming no unrelated main changes (base 155/136 plus 18).
- [x] Update compatibility and OKF with actual behavior, decisions and evidence. Use technical-documentation skill and refresh new source hashes after final edits.
- [x] Run the Windows whole workspace, all-target/all-feature Clippy and rustfmt checks. Fix default-dependent fixtures according to their actual contract, retaining default coverage where intended.
- [x] Run all new emitted-source witnesses on CPython 3.14 and 3.15; docs/knowledge pytest, generated reference checks and site build. Check disk and perform periodic targeted clean between completed phases.
- [ ] Complete the final independent whole-branch review and software-design-philosophy score. Record each of five implementation/test self-review passes and actual verification outcomes, with remaining limitations.
- [ ] Commit/push the reviewed branch, create a non-draft PR via gh, and resolve failing final-head CI. Confirm head and all workflows before reporting completion. Do not merge. Clean Cargo target and this plan's scratch after retaining public evidence; create no ZIP.

## Plan self-reviews

1. Coverage: 8+6+4 new IDs cover every adopted row, with the existing ignore operator strengthened in task 1.
2. Interfaces: one typed facade prevents the collector from gaining library-specific string decisions; task 2 owns shared policy protocol.
3. Dependencies: implementations are sequential; later tasks extend the already-reviewed facade and tests without parallel source edits.
4. Test evidence: each domain has public discovery, meaningful CPython behavior, negative identity/binding and property coverage. Runtime tests consume actual emitted edits.
5. Delivery/resources: focused red/green checks precede full validation; a single bounded Cargo queue and periodic clean prevent repeat disk exhaustion. PR and final-head CI are required, not optional follow-up.

## Implementation self-reviews

1. Text identity and source layout: kept nominal method matching separate from
   trusted imported APIs. Multipart normalization edits preserve the original
   argument text and comments while replacing the selected form.
2. Recognition resources: replaced multipart string joining with a bounded
   decoded-choice matcher. Cancellation polls include empty string parts, and
   delimiter requirements now belong to the domain facade.
3. Option timing: compared native truth conversion on both interpreters. The
   shared adapter supports repeated and skipped conversions; JSON custom encoder
   classes are excluded, and Base64 omission accounts for `ignorechars`.
4. Imported destinations and selection: direct URL aliases traverse the dotted
   import correctly. Review corrections track sibling-package writes and escapes
   and separate omitted-policy selection offsets from physical edit ranges.
   Normalized Unicode aliases that become keywords are excluded from emitted
   destination spellings, with the existing proof required for an alternative.
5. Numeric binding and source preservation: checked positional-only APIs and
   statistics center keywords against both interpreters. A self-review regression
   exposed comment removal inside a signed tolerance expression and required
   retaining its source when changing the tolerance.

## Test self-reviews

1. Each domain begins with failing named default/exclusion cases before operator
   registration. Cases distinguish absence of implementation from parser errors.
2. Runtime witnesses execute analyzer output and check the original in Arrange.
   This caught a URL property harness variable overwriting its import alias.
3. Encoding keyword-order witnesses use the non-signature order they claim to
   test. Properties cover parseability, candidate-cap prefixes and fixed-signature
   rejection independently of implementation details.
4. Resource regressions detect Ruff's literal cache without warming it in the
   assertion setup. Both cache and cancellation failures were reproduced before
   correcting the producer.
5. Shared changes rerun affected earlier domains. Independent selections test the
   callee and argument-delimiter lines separately; identity regressions cover both
   the source callable and its destination. Numeric layout cases include comments,
   grouped calls, implicit generators and CRLF.

## Verification outcomes

The unchanged merged baseline passed 5,276 `hoimin-python` unit tests with five
existing ignored fixtures. The initial filtered command matched no tests and is
not counted as validation. Before task 1 implementation, its public discovery
target recorded 19 expected failures and three passes: the eight new IDs were
unregistered, and existing ignore-policy discovery lacked three requested forms.

Direct CPython 3.14.9 and 3.15.1 probes confirmed that map and batched convert
strict at construction, parse_qs can test keep_blank_values once per blank field
or skip it entirely, and json.dumps can test allow_nan more than once. These
probes constrain the task 2 adapter; they do not replace emitted-mutant tests.

Task 1 completed at `7ed983e7`: 48 public analyzer cases and 23 emitted-source
runtime cases on each supported interpreter passed after the correction. Thirteen
focused module tests include the four memory/cancellation regressions demonstrated
failing before the fix. Independent scoped review approved the correction with
software-design score 10/10. Earlier full-library evidence predates that correction;
final workspace validation remains required. Periodic CLI package cleanup removed
213.8 MiB after the first verification phase.

Task 2 completed at `889d775d`. Review regressions first demonstrated sibling
package write/escape failures and incorrect line selection for split calls.
After the shared proof and selection-offset corrections, 5,195 library tests
passed with five existing ignores, together with 32 option and 48 text analyzer
cases. The 26 existing dynamic-boundary witnesses and 18 option witnesses passed
on each supported interpreter. Scoped re-review accepted both corrections and
scored all eight software-design diagnostics as satisfied.

Task 3 completed at `779369e9`. Its initial eight default/exclusion tests failed
before registration. A later layout regression reproduced comment removal from
a signed tolerance expression. Independent review also reproduced invalid Python
loads from Unicode aliases normalized to keywords. The corrections passed 53
numeric analyzer cases, 119 shared import cases and 28 numeric runtime cases on
each interpreter. The earlier shared-path checks passed all text, option and
dynamic-boundary runtime cases on both versions. Scoped re-review approved the
correction with all eight software-design diagnostics satisfied. Periodic CLI
package cleanup removed another 267.3 MiB during the review phase.

The documentation audit covers the added operator entries, compatibility notice
and knowledge sections. It passes all ten technical-documentation diagnostics:
the reference states effects and applicability, examples correspond to executable
witnesses, and the compatibility notice explains selection and saved-plan effects.
The existing CLI reference generator remains the source of the command reference.
The documentation and reference-tool tests passed 51 cases. Knowledge validation
found 65 well-formed pages, no missing links or sources, no unreachable pages and
matching hashes for both references to the new design.

## Integration validation

The fixed inventories contain 173 canonical IDs and 154 runtime defaults. Each
addition has a named saved-plan case that reloads the manifest and rediscovers all
selected candidates. Existing default-selection assertions remain in place.

The first Windows workspace run detected stale generated CLI help and an old
strict-encoding exclusion example. Regeneration and an exact-source expectation
for strict-to-ignore corrected those failures. A complete `--no-fail-fast` run
then found only the missing registrations in the initial Lean syntax-corpus
inventory. The 18 records now identify their domain's CPython witnesses; prior
corpus classifications and assertions remain unchanged. All 73 corpus tests
passed in the subsequent focused run.
The Windows run also emitted Git LF-to-CRLF conversion warnings from unchanged
synthetic missing-source fixtures; the scoped integration checks were quiet.

All-target Clippy identified type, structure and test-diagnostic issues. The
corrections give omitted and explicit encoding arguments named enum states,
isolate normalization emission, borrow URL identity and clarify numeric spans.
Assertions report the candidate count and input on failure. These are separate
from the earlier workspace run; the following checks cover the corrected code:

| Check | Result |
| --- | --- |
| `cargo clippy --locked -j 2 --keep-going --workspace --all-targets --all-features -- -D warnings` | Passed |
| `hoimin-python` library | 5,195 passed; five existing ignored fixtures |
| Standard-library analyzer targets | 53 numeric, 32 option and 48 text cases passed |
| Emitted-source witnesses, each of CPython 3.14.9 and 3.15.1 | 28 numeric, 18 option, 23 text and 26 existing dynamic-boundary cases passed; the `ignorechars` assertions execute on 3.15 |
| CLI reference generator | Ten tests passed; generated reference check passed |
| Documentation site | Build passed |
| Rust formatting and whitespace | Passed |

The final PR checks validate the published head independently of these local
verification runs.

The independent integration review approved the inventory, saved-plan tests and
lint refactors with no critical or important findings. Its design score is
8.75/10: seven diagnostics pass, while the exact 10–20% allocation of effort to
design was not measured. This is an evidence limitation, not a remaining code
change requested by that review.
