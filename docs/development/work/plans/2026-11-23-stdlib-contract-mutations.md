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
- Cancellation, selection, delimiter depth, payload budgets and candidate caps precede replacement materialization.
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

**Interfaces:** Produce `stdlib_contracts::Imports::build(module: &ModModule, operators: &MutationOperatorSelection, cancelled: &impl Fn() -> bool) -> Result<Self, AnalysisCancelled>` and `Imports::emit(&self, call: &ExprCall, site: &processing_calls::Site<'_>, emit: impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool)`. Domain producers own eligibility; root collector supplies a deferred sink. Module-private helpers may evolve in later tasks but do not expose domain identifiers as strings.

- [ ] Write public discovery/default/opt-out tests using operator IDs as strings before registration. A named case for each of unicode_digit_predicate_swap, unicode_digit_ascii, unicode_normalization_form, unicode_normalization_remove, string_casefold_lower, string_affix_strip, encoding_errors_replace, encoding_errors_strict must discover a candidate by default and none when excluded. Existing encoding_errors_ignore must gain omitted/positional/dynamic-policy cases.
- [ ] Root runs `cargo test --locked -j 2 -p hoimin-python --test stdlib_text_mutations`; capture intended missing-registration/behavior failures.
- [ ] Implement and register the eight new IDs, default list and ranking. Use private enums for predicate, normalization form, affix and error policy. Consolidate encoding recognition without duplicated candidate production. Keep valid existing ignored-policy cases.
- [ ] Add named runtime witnesses: fullwidth 2 rejected only by ASCII narrowing; superscript 2 distinguishes decimal/digit; fraction one-half distinguishes numeric; NFKC fullwidth 1 becomes ASCII; NFC/NFD composed accent differs; sharp-s lower versus casefold; `ababaX` prefix removal returns `abaX` versus strip `X`; invalid UTF-8 rejects/replaces/ignores; codec positional/keyword argument effects occur once in source order; custom nominal receiver behavior is explicitly tested/documented.
- [ ] Add properties for ASCII digit predicate invariance, admitted replacement parseability and cap-prefix stability. Add alias/shadow/duplicate-binding negatives and line-selection tests; group test setup helpers only when they hide useful mechanics.
- [ ] Record five implementation and five test self-reviews. Root runs focused analyzer + CLI witnesses, formatting and review diff. Update operator reference entries and commit the independently working task after task review.

## Task 2: Iterator and serialization option contracts

**Files:** Extend `stdlib_contracts.rs`; create `stdlib_contracts/options.rs`, `stdlib_contracts/urls.rs`, `crates/hoimin-python/tests/stdlib_option_mutations.rs`, `crates/hoimin-cli/tests/stdlib_option_witnesses.rs`. Reuse/refactor `dynamic_boundaries.rs` option rendering if the shared contract earns it. Update registry, ranking, collector and operator reference as needed.

**Interfaces:** Consume the task 1 Imports build/emit facade and processing Site. Domain-private typed API/signature descriptors encapsulate keyword roles. A shared Boolean-policy renderer owns omission insertion, explicit value replacement, grouping and native truth behavior; existing zip integration must retain its witnesses if changed.

- [ ] Write discovery/default/opt-out tests for map_strict_toggle, batched_strict_toggle, url_quote_swap, url_query_policy, base64_validate_toggle and json_allow_nan_toggle. Root runs the target before implementation to record failures.
- [ ] Implement the six IDs. Add omitted defaults; explicit Boolean literals remain handled by boolean_literal, with no duplicate candidate under the new ID. Validate signatures and default differences; JSON custom cls is excluded except None. Python 3.15 Base64 ignorechars must not falsely imply default validate=False.
- [ ] Runtime cases: map `[1,2]` with `[10]` truncates versus raises; batched length 3 size 2 emits final short tuple versus raises; quote `a b/c` distinguishes `%20`/`+` and slash; unquote `a+b`; urlencode list values; parse_qs empty value; b64decode `YQ==!`; json NaN; dynamic policy truth count/order before and during iteration; skipped truth paths and retained exception objects; one source scenario per invocation.
- [ ] Properties cover unchanged ASCII URL inputs without special characters, duplicate-option exclusion, candidate cap prefix and parsing of grouped generator/multiline calls. Validate shared zip behavior if refactored. Root runs focused suites and both interpreter witnesses.
- [ ] Record five implementation/test review passes, complete task review, update operator reference and commit.

## Task 3: Numerical and heap contracts

**Files:** Extend facade; create `stdlib_contracts/numeric.rs`, `crates/hoimin-python/tests/stdlib_numeric_mutations.rs`, `crates/hoimin-cli/tests/stdlib_numeric_witnesses.rs`. Update registry, ranking and operator reference.

**Interfaces:** Consume the existing facade and trusted import/destination helpers. Typed numerical API descriptors own signature differences including statistics center keywords; effectful original operands remain outside synthetic bodies.

- [ ] Write discovery/default/opt-out tests for math_isclose_tolerance, math_precision_loss, statistics_sample_population and heap_pushpop_replace. Run before implementation through root.
- [ ] Implement the four IDs and their documented contracts. Preserve function/argument evaluation; avoid arbitrary math function exchanges. Skip known-zero tolerances and omit the already-zero abs_tol default.
- [ ] Runtime witnesses: isclose(0,1e-10,abs_tol=1e-9) changes True to False; omitted rel_tol accepts a nearby nonzero pair but zero tolerance rejects; x=1e-16 exposes expm1/log1p precision loss; variance([1,2,3]) is 1 versus pvariance 2/3; keyword centers retain evaluation order; heap [5,9] and added 3 returns 3 versus 5 with corresponding heap state; empty-heap failure is intentional.
- [ ] Properties cover finite zero-tolerance isclose equality, population/sample relationship for at least two finite values, and heap multiset conservation across insertion/removal. Add aliases, function references that escape, argument-shape negatives, caps and line selection. Root runs focused targets with both CPython versions.
- [ ] Record five implementation/test review passes, complete task review, update reference and commit.

## Task 4: Integration, public evidence and delivery

**Files:** `docs/user/reference/cli.md`, `compatibility.md`, relevant core/CLI inventory fixtures, `docs/development/knowledge/design/analyzer.md`, knowledge index metadata as required, and this design/plan.

- [ ] Generate the CLI reference from the existing shared document source; validate IDs/default/exclusion/help/plan-verify discovery. Expected catalog is 173 and default runtime count is 154, assuming no unrelated main changes (base 155/136 plus 18).
- [ ] Update compatibility and OKF with actual behavior, decisions and evidence. Use technical-documentation skill and refresh new source hashes after final edits.
- [ ] Run the Windows whole workspace, all-target/all-feature Clippy and rustfmt checks. Fix default-dependent fixtures according to their actual contract, retaining default coverage where intended.
- [ ] Run all new emitted-source witnesses on CPython 3.14 and 3.15; docs/knowledge pytest, generated reference checks and site build. Check disk and perform periodic targeted clean between completed phases.
- [ ] Complete the final independent whole-branch review and software-design-philosophy score. Record each of five implementation/test self-review passes and actual verification outcomes, with remaining limitations.
- [ ] Commit/push the reviewed branch, create a non-draft PR via gh, and resolve failing final-head CI. Confirm head and all workflows before reporting completion. Do not merge. Clean Cargo target and this plan's scratch after retaining public evidence; create no ZIP.

## Plan self-reviews

1. Coverage: 8+6+4 new IDs cover every adopted row, with the existing ignore operator strengthened in task 1.
2. Interfaces: one typed facade prevents the collector from gaining library-specific string decisions; task 2 owns shared policy protocol.
3. Dependencies: implementations are sequential; later tasks extend the already-reviewed facade and tests without parallel source edits.
4. Test evidence: each domain has public discovery, meaningful CPython behavior, negative identity/binding and property coverage. Runtime tests consume actual emitted edits.
5. Delivery/resources: focused red/green checks precede full validation; a single bounded Cargo queue and periodic clean prevent repeat disk exhaustion. PR and final-head CI are required, not optional follow-up.
