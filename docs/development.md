# Development

Run the Rust quality gate locally with the same commands used in CI:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-cli --test run_e2e
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uvx maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

## Pinned Rust toolchain

`rust-toolchain.toml` pins the repository development and blocking CI
toolchain to Rust 1.98.0, including rustfmt and Clippy. Run `rustup toolchain
install` from the repository root to install that exact declaration. The
weekly latest-stable canary reports upcoming compatibility issues without
changing the blocking toolchain.

Updating this pin is a deliberate compatibility change: update
`rust-toolchain.toml`, run every quality-gate command above, and review the
latest-stable canary separately. Updating the repository pin does not raise the minimum supported Rust version.
An MSRV change follows the distinct procedure below.

## Minimum supported Rust version

`workspace.package.rust-version` in `Cargo.toml` is the minimum supported Rust
version (MSRV). CI checks the complete locked workspace with that compiler.
Run the same gate locally before updating Rust dependencies:

```console
rustup toolchain install 1.88 --profile minimal
cargo +1.88 check --workspace --all-targets --all-features --locked
```

The committed `Cargo.lock` must remain compilable on the MSRV. When a dependency
update raises its compiler requirement, select the newest dependency release
that still supports the MSRV. If the project deliberately raises its MSRV,
update `workspace.package.rust-version`, the `msrv` CI job, its workflow
contract test, and this section in the same pull request. Stable CI remains
required in addition to the MSRV gate.

Before running the standalone wheel smoke script, you must build a release wheel
first with `uvx maturin build --release`. Alternatively, set `HOIMIN_WHEEL`
to the exact path of an existing wheel to test. The script only selects and
tests an existing artifact; it does not build one.

## Reproduce randomized Rust test order

CI supplements the stable cross-platform suite with Rust's standard nightly
test harness in randomized order:

```console
rustup toolchain install nightly-2026-07-27 --profile minimal
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

The harness prints the generated seed. Replay a failing order exactly with:

```console
cargo +nightly-2026-07-27 test --workspace -- \
  -Z unstable-options --shuffle-seed <SEED>
```

The nightly job supplements rather than replaces the stable Ubuntu, Windows,
and macOS test jobs.

## Lean state-machine oracle

The formal model covers effect completion, stopping, cleanup, and final-report
lifecycle semantics. Lean proves properties of that model; it does not prove
the Rust implementation. A Rust adapter separately drives the public
`RunState` and `transition` API with the Lean-generated corpus to check their
correspondence.

Run the gates from the repository root in this order:

```console
(cd formal/HoiminOracle && lake build)
(cd formal/HoiminOracle && lake exe generate -- --check corpus/state-machine.jsonl)
cargo test -p hoimin-core --test lean_oracle
HOIMIN_ORACLE_CASE=cleanup_is_emitted_once \
  cargo test -p hoimin-core --test lean_oracle \
  oracle_correspondence -- --exact --nocapture
```

The generated JSONL corpus is owned by Lean and must not be edited by hand.
New cases begin in report mode while their model/implementation boundary is
reviewed. Promoted strict cases are blocking; infrastructure errors always
fail. Corpus generation, freshness, and strict correspondence are separate,
deterministic command boundaries suitable for a future CI job. This workflow
does not currently modify or require a CI configuration.

## Shutdown deadline invariants

Once total timeout, cancellation, or a fatal failure starts shutdown, the
first cause and its absolute shutdown deadline are immutable. Total timeout is
anchored to the original run deadline plus the fixed two-second grace;
cancellation and fatal failure receive the same grace from their first
observation. Later stop signals or failures must not restart or extend that
budget. Every cancellable post-stop scheduler wait, completion receive, process
drain, blocking-I/O drain, resource close, and internal metrics finalization
must use the same deadline.

At grace expiry, accept already-buffered completions before aborting Tokio task
wrappers so returned workspace ownership is recovered when possible. Do not
claim cleanup, session completion, or `run_finished` unless its completion was
accepted before expiry. Resource ownership still held by the shell is moved to
a detached blocking cleanup rather than leaked; that cleanup may finish after
the run future returns and is never reported as accepted. Tokio cannot cancel
an already-running `spawn_blocking` operation: aborting its wrapper detaches
that operation. The CLI exits after reporting the infrastructure failure,
while a library caller may observe the detached operation finish later. This
deadline cannot preempt
an arbitrary synchronous `Write`: report output and its flush run inline, so a
blocked caller-provided writer can delay the run future and even the expiry
diagnostic. The bounded-return invariant therefore assumes synchronous output
writes make progress.

## Analyzer timeout invariants

Plan creation and verification rediscovery apply the normalized
`--analyzer-timeout` as one absolute deadline for the complete discovery phase;
the deadline is not restarted for each target. Both plan entry points report
the same `plan.discovery: analyzer.timeout` diagnostic and exit 2. Plan
creation produces no manifest, and verification does not start the test
command.

Discovery runs as an owned blocking task with cooperative cancellation. When
the deadline expires, the async caller returns promptly and detaches an analyzer
that is already running; the detached task observes cancellation, stops, and
releases every resource it owns. Tokio cannot preempt synchronous work already
executing in that task. In particular, a source read blocked inside a system
call must return before cancellation can be observed and the detached task can
finish.

## Extending collection and structural mutations

The Rust analyzer keeps token-local mutations in its token scanner and adds an
AST candidate pass for calls, literals, subscripts, and slices. Both passes
emit the same candidate form and use the shared selection, profile filtering,
deduplication, source ordering, and candidate-limit pipeline. Keep new
structural rewrites to one contiguous AST span; build the replacement from the
original source text so nested expressions, comments, and spelling are
preserved.

Raw-token replacements are restricted to the AST-proven token-start allowlist
recorded by `AstFacts`. It records only the operator spellings in the precise
AST gaps for their supported roles, such as comparisons, boolean and binary
expressions, unary expressions, augmented assignments, boolean literals, and
`break`/`continue`. A token with an ambiguous grammatical role is skipped
conservatively: matching text alone is never enough to make it a candidate.
Every approved raw-token operator is then checked against the annotation
containment index and skipped when its complete token range lies inside an
annotation. Deliberate annotation mutations remain in the separate opt-in
`type_*` producer. These gates do not change the existing selection, profile
filtering, deduplication, source ordering, or candidate-limit behavior.

For one source file, the token scanner, AST pass, and type-annotation pass
each retain no more than `max_candidates + 1` candidate records and
deduplication identities before their bounded merge. This is deliberately a
candidate-retention bound, not a general Hoimin memory bound: source text,
parser tokens, AST facts, and small per-node replacement lists remain
proportional to source size. `--max-memory` controls descendants rather than
the Hoimin CLI, so it does not bound these analyzer structures.

Candidate-local punctuation queries must use
`AstFacts::candidate_tokens_in_range` with the smallest relevant AST range.
Do not scan `Tokens::iter()` for each call, literal, or exception tuple: that
turns a module containing many small candidates into quadratic work. The
test-only candidate token lookup statistics count calls and tokens in the
returned slices. Treat their operation-count bound as the complexity contract;
wall-clock timings are diagnostic only.

`AstFacts` finalizes immutable lookup indexes after its AST walk. Annotation
and focused-profile arid containment use sorted starts with prefix-maximum end
offsets, so overlapping ranges remain exact with `O(log n)` lookup. Unary
`not` operands use an exact-start hash lookup with amortized `O(1)` access.
Definition ranges are swept into disjoint innermost-scope segments and queried
by binary search in `O(log n)`. Do not replace these with a raw-vector fallback
or make lookup correctness depend on token/visitor call order.

Property tests compare the indexes with independent linear definitions for
overlap, nesting, gaps, equal starts, and half-open boundaries. The ignored
adversarial benchmark covers hundreds of scopes and annotations plus thousands
of unary `not` and focused arid facts:

```console
cargo test --release -p hoimin-cli --lib \
  benchmark_adversarial_ast_fact_indexes -- --ignored --nocapture
```

It asserts exact candidates and logarithmic comparison ceilings through
test-only counters. Elapsed time is printed for profiling but is deliberately
not a test threshold. When adding a fact query, extend the semantic comparison
and operation-count assertions rather than introducing a timing-sensitive CI
gate.

Bare builtin calls (`any`, `all`, `list`, `tuple`, `set`, `frozenset`, `min`,
`max`, `sorted`, and `reversed`) use scope-aware shadowing checks. A pair is a
candidate only when both its source and replacement names definitely resolve
through Python's builtins namespace at that occurrence. The resolver follows
whole-function local binding, module/class source order, and closure lookup.
It also preserves class non-closure, `global`/`nonlocal`, and the comprehension
leftmost-iterable boundary. Wildcard imports, conditional bindings, deletions,
bare `exec`/`globals`/`locals`/`vars` calls, missing occurrence facts, and other
ambiguous cases are `Unknown` and suppress the candidate. Qualified builtin
calls are not candidates. Method mutations are syntax-directed and do not
infer receiver types.

The analyzer emits a `split`/`rsplit` swap only when the call supplies a
second positional argument or the named `maxsplit` keyword. Calls that omit
`maxsplit` produce identical string results, so the analyzer skips them.

Type-annotation collection records an import-state snapshot at each annotation
site in source order. Signature annotations use their enclosing state, while a
function body predeclares Python-local names before its body is visited. Nested
scopes do not leak their bindings into an enclosing scope. At control-flow
joins, retain only imports known identically on every reachable exit; when no
safe direct-name or module-alias spelling remains, skip the replacement.

The supported structural shapes are exact: `append(value)` ↔
`extend([value])` only when the inverse list literal has one non-starred
element; `mapping.get(key)` ↔ `mapping[key]` only for a simple name or
attribute receiver, one positional key, and load context; `sort()` ↔
`reverse()` only with no arguments; and `sorted(value)` ↔ `reversed(value)`
only with one positional argument and no keywords. Calls with unsupported
keywords, star arguments, defaults, trailing commas where a rewrite would be
ambiguous, complex mapping receivers, or target contexts are skipped.

Boundary mutations likewise use only load-context subscripts. An index must be
a plain decimal integer literal: emit `+1`, and also `-1` when positive.
For a slice, plain decimal start, stop, and step literals may move to adjacent
valid values, except a step mutation to zero. Negative, empty, non-decimal, and
expression bounds are excluded. Comprehensions, assignment/delete targets,
the `append`/`pop` pair, and set literals wrapped as `frozenset(...)` are not
supported transformations.

Regression tests should use `apply_candidate_and_reparse` to replace the
candidate's one span in its source and verify
`ruff_python_parser::parse_module` accepts the result. This is a test-only
invariant; production does not parse each candidate separately. Keep exact
candidate/replacement assertions alongside this parse-preservation check.

Run focused analyzer tests while changing these rules:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::shadowed_collection_builtins_are_not_mutated_as_calls -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::collection_calls_and_literals_emit_exact_parseable_candidates -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::structure_calls_emit_exact_parseable_candidates -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::structure_index_neighbor_mutates_decimal_load_indices_only -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::structure_slice_neighbor_mutates_decimal_bounds_without_zero_steps -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests
```

## Extending Python exception mutations

Python exception candidates are collected by the Rust analyzer's
`ExceptHandler` AST pass and its `Stmt::Raise` statement pass, then use the
shared selection, profile, source ordering, deduplication, and candidate-limit
pipeline. The default `exception_type_pair` operator replaces only simple,
unqualified handler names or the simple primary name of a supported raise
expression with a curated counterpart: `ValueError`/`TypeError`, `KeyError`
with `IndexError` and `AttributeError`,
`FileNotFoundError`/`PermissionError`, `ConnectionError`/`TimeoutError`,
`ImportError`/`ModuleNotFoundError`, and
`ZeroDivisionError`/`OverflowError`.

`AstCandidateCollector` must enter its exception-type context for both `except`
and `except*`, keep that context balanced, collect dedicated exception
candidates outside the generic collection gate, and visit handler bodies
normally. This keeps collection mutations out of type positions, where Python
requires an exception class or a tuple of exception classes, including generic
`list`/`tuple` constructor-call and literal candidates nested in the type.

Exception names use the same scope-aware resolver as builtin-call pairs. Both
the handler's source name and each inserted or replacement name must definitely
resolve through Python's builtins namespace. Assignments, imports, parameters,
comprehension and match captures, and `except ... as` targets suppress only
occurrences where their binding is visible; sibling scopes do not leak. Class
targets are not closure bindings for methods. Wildcard imports and ambiguous
control flow remain conservative `Unknown` results. Qualified and dynamic
handler types, `except*`, and tuple members outside the curated built-in name
set are skipped.

For a raise statement, `AstCandidateCollector::visit_stmt` inspects only
`StmtRaise.exc`. It accepts `raise ValueError` and calls whose callee is a
simple name, such as `raise ValueError(message)`. The replacement span is only
that name, so arguments and `raise ValueError(...) from cause` remain intact.
Normal AST walking then visits the primary expression and cause exactly once,
preserving candidates from other selected operators without interpreting the
cause as an exception type. Bare re-raise, qualified names, dynamic callees,
shadowed source or destination names, and termination exceptions are skipped.

The five structural operators in `exception_risky` are explicit-only:
bare-handler insertion/removal, the `Exception`/`BaseException` boundary, and
curated tuple add/remove rewrites. Tuple candidates use parser token ranges and
the original source text so commas, comments, trailing commas, and line endings
remain intact and every replacement can be reparsed. The BaseException boundary
can change handling of `SystemExit`, `KeyboardInterrupt`, and `GeneratorExit`,
so it must not be added to the default selection.

When changing these rules, keep exact candidate and replacement assertions next
to `apply_candidate_and_reparse` checks. Run the focused tests before the full
analyzer module:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_type_pair_candidates_are_curated_and_syntax_directed -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::raise_exception_type_pair_candidates_preserve_the_primary_expression -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::raise_exception_type_pairs_use_scope_aware_source_and_destination_resolution -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_risky_candidates_require_explicit_selection_and_reparse -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::sibling_bindings_do_not_suppress_builtin_or_exception_pairs -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::comprehension_exception_target_and_wildcard_boundaries_are_conservative -- --exact
```

## Extending plan ranking

Plan manifests use schema version 2 and ranking rule version 3. The schema
version describes the manifest's serialized shape; the ranking rule version
describes the category and scoring semantics used to order its candidates.
Change the ranking rule version whenever those semantics change, even when the
manifest schema itself does not.

Every canonical mutation operator is exhaustively assigned to exactly one
fixed-score category:

- `high_value_control` (100): comparisons, membership and identity tests,
  boolean operations, `not`, boolean literals, and `break`/`continue`.
- `exception_handling` (90): safe and risky exception-handler mutations.
- `behavioral` (80): collection and structural behavior mutations.
- `arithmetic` (70): arithmetic, unary-sign, and bitwise mutations.
- `type_annotation` (50): type-annotation mutations.

Keep the `MutationOperator` match exhaustive instead of adding a fallback arm.
This makes a new operator fail compilation until its ranking category has been
chosen. Unknown external operator strings remain invalid and do not acquire a
generic category.

`plan::create` validates its completed manifest with the same header validator
used by `verify` before returning it. Preserve this output-boundary invariant:
generated plans must satisfy schema, source-record, ranking, candidate-ID, and
normalized-path checks rather than deferring an internal inconsistency until a
later verification command.

## Test provenance comments

Use `// pins: issue #NNN` only when an assertion intentionally preserves a
surprising policy or a former defect whose expected result is not self-evident.
Do not annotate ordinary exact assertions.

Place the comment immediately before the assertion or operation whose outcome
needs that context. Prefer the concrete former-defect issue over an umbrella or
property-testing issue. Do not annotate generators, fixture constructors, or
routine schema values, and do not backfill unrelated tests mechanically.

## Verify delegated cgroup v2 CI

CI runs `linux-cgroup-v2-hard` only for a push to `main` when the repository
variable `HOIMIN_CGROUP_V2_DELEGATED` is `true`. The matching self-hosted runner
must be online and carry all of these labels: `self-hosted`, `linux`, `x64`,
and `cgroup-v2-delegated`.

Find the main-push run and inspect its jobs and delegated log:

```console
gh run list --workflow CI --event push --branch main
gh run view <RUN_ID>
gh run view <RUN_ID> --job <JOB_ID> --log
```

Successful hard-backend evidence requires `linux-cgroup-v2-hard` to complete
and its `Require delegated cgroup v2 hard tests to run` log to contain no
`SKIP:` marker. If no online idle runner matches every label, GitHub leaves the
job queued until a matching runner becomes available and fails it after 24
hours. Check the repository or organization Actions runner page for runner
status and labels; do not replace the job with a hosted or best-effort runner.

After changing the Rust analyzer or its tests, use the bounded workflow below
to collect focused evidence. For the required complete Rust inventory, run
`cargo mutants --workspace` as described in the Rust mutation testing section.

## Focused 30-minute Rust mutation workflow

Use `tools/focused_mutation.py` to collect bounded evidence about the
highest-ranked Rust changes. It requires `cargo-mutants` 27.1.0. Run it from
the repository worktree and put its artifacts outside the repository:

```console
output_dir="$(mktemp -d /tmp/hoimin-focused-run.XXXXXX)"
uv run --frozen python tools/focused_mutation.py \
  --budget 30m \
  --base origin/main \
  --output "$output_dir"
```

For a fixed path in automation, the equivalent output argument is
`--output /tmp/hoimin-focused-run`. Do not name the output directory with a
`mutants.out` prefix.

By default, the tool discovers eligible Rust files changed from `--base` and
ranks their cargo-mutants inventory. Repeat `--file PATH` and `--symbol NAME`
to explicitly focus discovery; either selector can be supplied more than once.
Add `--iterate` only when deliberately reusing cargo-mutants' prior caught and
unviable results during test development. The 30-minute budget reserves the
last five minutes for checkpointing and reporting, so it stops starting
mutations at that boundary.

`run.json` is checkpointed throughout. `run.json` remains the recoverable
machine-readable source of truth. Per-command arguments, stdout, and stderr are
also retained below the same output directory. `report.md` is generated or
refreshed during finalization as the human-readable summary. The tool attempts
finalization after a timeout, handled interruption, baseline failure, or tool
error, but `report.md` may be absent after an abrupt unhandled process
termination before finalization. In that case, recover from `run.json` and the
command artifacts. Treat incomplete output as a partial report: candidates
marked `not_run`, `pending`, `timeout`, `unviable`, or `error` remain
unverified.

Exit code `0` means the run reached `completed` or `budget_exhausted`; the
latter is an expected bounded result, not evidence that every candidate ran.
Exit code `130` means interruption. Exit code `2` means a configuration,
baseline, cargo-mutants, command, or reporting failure. A killed mutant is
evidence that the selected test command detects that change. A survivor is not proof of a bug.
Manually classify each survived result by inspecting the exact mutation,
relevant production behavior, tests, and recorded command artifacts.
Also investigate timeouts, unviable mutants, errors, and unverified candidates
instead of treating them as passes.

To measure how much a focused run reduced the candidate set, pass a compatible
previous full `run.json` with `--prior-inventory PATH`. The report then compares
the focused candidate count with that inventory; without it, the reduction
ratio is explicitly unmeasured. This comparison does not make a focused run a
complete inventory.

Focused results guide short test-improvement loops, but release evidence still
requires the full command below. Run `cargo mutants --workspace` after the
focused work, and do not use `--iterate` for the required final inventory.

## Rust mutation testing

Install `cargo-mutants` locally, then use the full command to discover every
outcome. While adding tests, `--iterate` reuses previously caught and unviable
outcomes; do not use it for the required final check.

```console
cargo install --locked cargo-mutants --version 27.1.0

# Discover all remaining outcomes.
cargo mutants --workspace

# While adding tests, reuse caught and unviable outcomes from the prior run.
cargo mutants --workspace --iterate

# Required final check: do not use --iterate here.
cargo mutants --workspace
```

The required reproducible priority check was validated with `cargo-mutants`
27.1.0:

```console
cargo mutants --workspace --jobs 4 \
  --file crates/hoimin-core/src/machine.rs \
  --file crates/hoimin-core/src/target.rs \
  --file crates/hoimin-cli/src/cli.rs \
  --file crates/hoimin-cli/src/process/mod.rs \
  --re '(TryFrom<Command> for ParsedCommand>::try_from|parse_bytes|raw_config|ProcessHandler::run|ProcessStartGate::cancel|ProcessCancellation::cancel|RunState::accept_completion|RunState::schedule_read_or_finalize|targets_are_normalized|changed_is_normalized)'
```

`mutants.out/missed.txt` requires a behavior test unless the exact mutant is
equivalent. Resolve `timeout.txt`, a failed baseline, and tool errors;
`unviable.txt` is inconclusive. Each allowed exception is an anchored
complete-name `exclude_re` with a TOML reason comment. This workflow is local
and does not run in CI.
