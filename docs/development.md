# Development

## Knowledge workflow

Start development work with [the OKF catalog](knowledge/index.md). Read the
relevant contracts and audit limitations, then inspect their sources and the
affected implementation. Update the corresponding OKF concepts in the same
change when behavior, design decisions, procedures, or audit evidence change.
Add a new concept when the work establishes a distinct reusable topic.

Follow [the OKF authoring and review procedure](okf-workflow.md) for update
criteria, source provenance, and validation. Include consulted pages, updated
pages (or the reason no update was needed), and checks performed in the PR or
final handoff. These steps also apply when no personal `create-okf` skill is
installed.

## Resource policy at the core boundary

Select the process backend before constructing `RunState`. Pass its
`ProcessHandler::resource_control()` description as the final argument to
`RunState::new`, `with_fingerprint`, `with_candidate_filter`, or
`with_ordered_candidate_filter`. `RunStarted::minimal` also requires an explicit
`ResourceControl` as its final argument. Rust callers using the older constructor
signatures must supply this argument; there is no inferred or default backend.
Tests that supply a policy exercise core reporting, not native OS enforcement.
The report schema retains its existing `mode` and singular `mechanism` fields.

## Local quality gate

Run the Rust quality gate locally with the same commands used in CI:

```console
cargo fmt --all -- --check
cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings
cargo test --workspace
cargo test -p hoimin-cli --test run_e2e
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uvx maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

## CI platform execution policy

Pull requests and pushes to `main` run the repository gates on Linux. Linux CI
consumes runner capacity; it is the platform deliberately kept automatic.
Windows and macOS validation lives in the separate `Manual non-Linux CI`
workflow and starts only through `workflow_dispatch`. Its results are not a
dependency or merge condition for automatic Linux CI.

After the workflow exists on the default branch, run it once against the final
ref that needs non-Linux evidence:

```console
gh workflow run non-linux-ci.yml --ref <REF>
```

Do not dispatch it for intermediate commits. The tag-triggered release workflow
is separate from this validation policy.

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
contract test, and this section in the same pull request. The pinned stable CI
gate remains required in addition to the MSRV gate.

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

The nightly job supplements rather than replaces the automatic stable Ubuntu
jobs. Windows and macOS validation uses the manually dispatched workflow
described above.

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
fail.

The Linux `Lean audit` CI job is configured to compile all 121 package modules
serially before building the aggregate library. It then runs all 29 corpus
freshness checks and the 26 generators that expose sensitivity gates. The
workflow is the canonical list of package targets, corpus paths, and gate
commands; update its contract test whenever a library module or `lakefile.toml`
executable changes.

Every build and generator invocation runs alone through
`tools/lean_resource_guard.py`, with a 30-second wall-time limit, 2 GiB
aggregate RSS limit, and 250 ms sampling. Resource statistics are retained as
the `lean-audit-stats` workflow artifact even when a gate fails. The Lake package
uses `-j1` and `-DElab.async=false` to keep elaboration serial within each
process. Run local Lean checks serially with the same guard rather than starting
an unbounded aggregate build. Corpus freshness checks compare generated output;
regenerate through the corresponding `lake exe generate* -- --output ...`
command when an intentional model change requires it. Generated JSONL files
must not be edited by hand.

The earlier 171-command sequence passed from an empty build cache in a
one-CPU Linux aarch64 container with a hard 2 GiB limit and no swap. The longest
command took 13.198 seconds; peak aggregate RSS was 1,077,976 KiB. Budget
statistics take their exploration depth at runtime and run only for `--stats`,
so corpus checks do not initialize the exhaustive statistics search. The
statistics formulas, depth-six audit, proofs, and corpora remain unchanged.
The initial GitHub-hosted run exceeded the former 20-second limit while
building `ShutdownProofs` at 1,022,208 KiB peak RSS. The current 30-second limit
retains the 2 GiB memory bound; the local measurements above used 20 seconds.
The hosted follow-up passed all 171 commands. `ShutdownProofs` was the longest
at 24.625 seconds and also had the highest RSS at 1,033,644 KiB.

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

## Metrics destination permission

The owned blocking Preflight validates metrics output against resolved source
entries, explicit fingerprint inputs and session artifacts before the baseline.
Its completion returns a typed destination together with workspace ownership;
the shell accepts both before applying the core event. The finalizer uses only
that resolved path. A collision failure can otherwise return a modeled nonzero
result and reach finalization, so checking `run_result.is_ok()` cannot authorize
output. Cleanup completions preserve the preflight decision. A later baseline
failure retains authorized metrics; a Rust error retains `metrics.incomplete`.

Compare the parent directory and native entry name for rename replacement.
Distinct hardlinks and final symlinks remain separate output entries. Unix
inspection caches directory names and lazily indexes no-follow file identities
to resolve inexact spellings; file identity alone does not equate outputs. The
session artifact resolver supplies the canonical database, its companion files,
and the ownership directory used by SessionHandler on every platform. Protect
that literal directory entry and its resolved tree. Inspect the originally
configured database entry before preflight replaces the session path with its
canonical path. On Windows, also retain protection of companion names under the
configured basename when the final entry is an alias.

For prospective ASCII case aliases in one directory, query macOS pathconf,
Windows directory case information, or the ext4/f2fs casefold flag on Linux.
Unknown filesystem behavior, ambiguous entry aliases and prospective non-ASCII
comparisons with protected names withhold metrics and produce a late
`metrics.write` warning. Unsupported Windows trailing-dot, trailing-space,
stream and prospective short-name spellings follow that policy. Keep ordinary
directory or missing-parent output failures as warnings. Require an existing
resolved parent before granting write permission, even if a baseline might
create the directory later. A prospective ownership tree can still establish
a collision before this permission check. Preserve directory-only output syntax
before path normalization: a trailing separator or terminal `.` / `..` withholds
metrics, so normalization cannot turn a directory requirement into replacement
of its final symlink. These checks establish
preflight destination identity; they do not freeze the filesystem against
external renames during a run. Native platform tests must establish the path
semantics; abstract lifecycle oracles alone cannot do so.

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
expressions, unary expressions, augmented assignments, boolean literals,
`True`/`False` singleton match patterns, and `break`/`continue`. The pattern
traversal admits boolean singletons inside nested patterns but excludes `None`,
wildcard and capture patterns, and string values. A token with an ambiguous
grammatical role does not produce a candidate: matching text alone never grants
eligibility.

Mapping-pattern boolean flips and complex separator flips are excluded when
only that edit would duplicate another literal key in the same mapping. A
per-mapping hash index models Python equality for the replacement domain:
boolean targets are exactly 0/1, while nonzero imaginary parts only compare
against complex keys. Zero-imaginary separator edits preserve equality and
remain eligible. Ordinary integer keys are never rounded for comparison;
integer real parts of complex literals use `num-bigint` conversion to match
Python construction, including radix spelling and ties-to-even rounding.
Float infinity and signed zero are handled separately from integer conversion
overflow, which cannot form a valid original complex pattern literal.
Nested mappings, value patterns, and dictionary expressions retain their own
eligibility. Integration tests compile public plan mutants with CPython and
check that import-only runs do not count invalid duplicate-key edits as kills.

Native Python operator syntax uses these token-local mappings:

- `**` becomes `*` (`binary_power`) and `@` becomes `*` (`binary_matmul`).
- `^` becomes `&` (`bitwise_xor`) and unary `~` becomes unary `+`
  (`bitwise_invert`).
- `**=` and `@=` become `*=` (`augmented_power` and `augmented_matmul`).
- `&=` and `|=` exchange spellings (`augmented_bitwise_and_or`), `^=` becomes
  `&=` (`augmented_bitwise_xor`), and `<<=` and `>>=` exchange spellings
  (`augmented_bitwise_shift`).

The binary, unary, and augmented-assignment AST roles admit these spellings only
in their operator positions. Decorators, keyword unpacking, annotations,
strings, and comments remain excluded.

`operator_function` is a default runtime selector for the Python 3.14 `operator`
callables (53 canonical names and 46 documented dunder aliases). The independent
`OperatorImports` index trusts only unique, unconditional module-level imports:
`import operator`, module aliases, and absolute `from operator import ...`
aliases. It does not alter the audited builtin resolver. Callable references are
eligible in both direct calls and higher-order uses such as `map(op.add, xs, ys)`.

| Callable names | Replacement |
| --- | --- |
| `eq` / `ne`, `lt` / `le`, `gt` / `ge` | Exchange each pair |
| `add` / `sub`, `mul` / `truediv`, `floordiv` / `mod` | Exchange each pair |
| `pow`, `matmul` | `mul` |
| `and_` / `or_`, `lshift` / `rshift` | Exchange each pair |
| `xor` | `and_` |
| `neg` / `pos` | Exchange |
| `abs` | `neg` |
| `index`, `inv`, `invert` | `pos` |
| `not_` / `truth`, `is_` / `is_not`, `is_none` / `is_not_none` | Exchange each pair |
| `iadd` / `isub`, `imul` / `itruediv`, `ifloordiv` / `imod` | Exchange each pair |
| `ipow`, `imatmul` | `imul` |
| `iand` / `ior`, `ilshift` / `irshift` | Exchange each pair |
| `ixor` | `iand` |
| `concat` / `iconcat`, `countOf` / `indexOf` | Exchange each pair |
| `getitem` | `contains` |
| `contains` | `(lambda container, item, /: item not in container)` |
| `setitem` | `(lambda container, key, value, /: None)` |
| `delitem` | `(lambda container, key, /: None)` |
| `call` | `(lambda target, /, *args, **kwargs: None)` |

For function pairs through a module alias, only the member token changes. A
dunder source keeps dunder spelling when the destination has a documented alias;
for example, `__not__` becomes `truth`. A from-import reference becomes
`__import__('operator').replacement` only when the builtin `__import__` has no
binding or namespace uncertainty. Lambda mutations replace the complete callable
reference. All forms retain argument text, order, count, and evaluation frequency;
lambdas intentionally suppress the underlying operation. Callable identity and
introspection are not preserved. Helpers `attrgetter`, `itemgetter`,
`methodcaller`, and `length_hint`, undocumented aliases, and functions outside
the documented Python 3.14 inventory are excluded. The Python 3.14 identity
predicates have no documented dunder aliases; names such as `__is_none__` and
`__is_not_none__` are not candidates.

The index invalidates an imported name if any other binding anywhere in the
module affects it, including parameters, definitions, type parameters, imports,
assignment/deletion targets, comprehensions, walrus expressions, exception and
pattern captures, and `global`/`nonlocal` declarations. Relative, conditional,
and local imports are not trusted. References before the import are excluded,
including function bodies defined before the import even if callers would run
them afterward. Stores, annotations, and match patterns do not produce callable
candidates; pattern grammar cannot generally accept the replacement expressions.

All imported-alias loads evaluated in a class namespace are excluded. A custom
or inherited metaclass can supply even ordinary names through `__prepare__`
without an AST assignment target. The exclusion includes class-body statements
and method decorators and defaults; it deliberately skips safe class-namespace
loads instead of attempting metaclass-provenance inference. Ordinary method and
lambda bodies use function/global lookup and remain eligible. Class-body list,
set, and dict comprehensions and generator expressions likewise use their
implicit function scope for targets, filters, later iterables, and produced
expressions. Their leftmost iterable remains a class-namespace lookup and is
excluded. Import aliases
beginning with `__` remain excluded throughout class-definition ranges,
including method bodies and nested functions, because private-name mangling and
compiler-provided names such as `__class__`, `__module__`, and `__qualname__`
can resolve them to other objects.

Wildcard imports, dynamic namespace operations, explicit `__dict__`/`vars`
access, and writes to builtin `__import__` invalidate namespace certainty.
Taking references to namespace operations (`exec`, `eval`, `globals`, `locals`,
`vars`, `setattr`, `delattr`, `__import__`) also counts, including qualified
attributes and aliases imported from `builtins` or `operator`. Namespace
dunders (`__dict__`, `__setattr__`, `__delattr__`, `__getattribute__`) are also
uncertain, including from-import aliases. `sys.modules` access and imports
are conservative exclusions. Attribute names associated with these operations
are treated as uncertain even on other objects. A write/delete through any
imported `operator` module alias, including local aliases, invalidates all
operator imports. Letting a module alias escape through assignment, argument
passing, or another bare load also invalidates all operator imports, avoiding
unsound assumptions about mutation through another alias. Ordinary writes such
as `self.value = value` do not invalidate independent operator bindings.

These checks are intra-module and deliberately conservative. They assume normal
standard-library imports, do not resolve project import search paths, and do not
prove absence of external monkey-patching or custom import loaders. The index
traversal checks cancellation, and its candidates enter the existing bounded AST
producer, selection, profile filtering, and deterministic merge.

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

Before facts, name resolution, or annotation analysis, `rust/depth.rs` checks
AST depth using a borrowed `AnyNodeRef` worklist. The module has depth 1; every
child exposed by Ruff's source-order visitor adds one, including auxiliary
nodes. The supported limit is 128. For `value = 1+...+1`, 126 terms reach depth
128 and 127 terms exceed it. Rejection is a typed `AnalysisError::DepthExceeded`,
mapped by both analyzer callers to `analyzer.depth` with the target path.
Cancellation remains a separate error. Plan creation fails without a manifest;
run reports incomplete analysis after its normal baseline stage.

The parser is a local backport of `littrs-ruff-python-parser 0.6.2`, selected via
the root Cargo patch. Ruff PR #25464's recursive checkpoints use `stacker`
(locked to 0.1.25): each checks a 128 KiB red zone and grows a 1 MiB segment when
needed. Context assignment, assignment/delete validation, old-decorator checks,
and pattern-to-expression recovery have checkpoints too. This preserves the
grammar and syntax diagnostics before applying Hoimin's AST-depth limit, without
assuming that one larger worker stack is sufficient for every input.

The parser's retained/unchecked module API preserves partial invalid trees so
Hoimin can dispose of them safely while keeping existing invalid-syntax
behavior. Rejected, invalid, and preflight-cancelled trees use an owned worklist:
Ruff's `Transformer` detaches statements, expressions, patterns, and interpolated
string elements before their shallow shells drop. These four callbacks cut every
recursive cycle in the pinned Transformer's traversal, including nested format
specifications. This disposal implementation lives in the parser's `ast_cleanup`
module and also releases discarded with/match speculative results, invalid
keyword patterns, and discarded `as` subpatterns. Hoimin's `depth::dispose` calls
the same implementation. Accepted trees use ordinary drop after passing the depth check.
Both worklists can allocate in proportion to input size; they are stack-depth
controls, not general memory bounds.

Regression coverage includes exact accepted/rejected boundaries with actual
binary and annotation candidates on a 2 MiB thread, auxiliary AST kinds,
repeated 25,000-term rejections with retained-allocation checks, and debug/release
public plan/run subprocesses at 2,000, 20,000, and 25,000 terms. The 20,000-term
fixture previously overflowed during ordinary AST drop alone. These observations
validate the selected limit on tested stacks; they do not prove arbitrary parser
inputs or every platform stack safe. In particular, the guard runs after parsing.

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
unqualified handler names in either `except` or `except*`, or the simple
primary name of a supported raise expression, with a curated counterpart:
`ValueError`/`TypeError`, `KeyError` with `IndexError` and `AttributeError`,
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
handler types and tuple members are skipped by the safe pair operator. The
starred handler path invokes only this safe simple-name collector; its
`Exception`/`BaseException`, bare-handler, and tuple rewrites remain disabled
even when the explicit-only risky operators are selected.

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

Plan manifests use schema version 4 and ranking rule version 4. The schema
version describes the manifest's serialized shape; the ranking rule version
describes the category and scoring semantics used to order its candidates.
Change the ranking rule version whenever those semantics change, even when the
manifest schema itself does not.

An explicitly selected symbol contributes 250 points to that symbol and its
dot-delimited descendants in the same resolved file. The bonus is awarded once,
even when both a parent and child selector match; `Box` matches `Box.check`, but
does not match `BoxOther.check`. Verification rejects plans with older ranking
rules and directs the user to regenerate them.

Every canonical mutation operator is exhaustively assigned to exactly one
fixed-score category:

- `high_value_control` (100): comparisons, membership and identity tests,
  boolean operations, `not`, boolean literals, and `break`/`continue`.
- `exception_handling` (90): safe and risky exception-handler mutations.
- `behavioral` (80): collection, structural, and `operator_function` callable
  and protocol behavior mutations.
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

## Rust test workflow

Use the normal Rust tests, contract tests, and Lean oracle checks to validate
changes to the Rust implementation. Select a package, integration test, or test
name when investigating one behavior, then run the workspace checks before
merging:

```console
cargo test -p hoimin-cli --test analyzer_handler
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
```

Rust mutation testing with cargo-mutants has been removed because its memory
and disk consumption is unsuitable for this repository's development workflow.
Do not install or invoke it for repository checks. Earlier audit reports and
implementation plans describe historical runs and are not current instructions.

Hoimin's Python mutation testing remains available through `hoimin plan` and
`hoimin verify`; see the repository's Python mutation-testing skills for target
selection and resource limits.

## Worker import roots

`RawRunConfig.import_roots` normalizes to an ordered, duplicate-free list in
`RunConfig` and `PlanConfig`, separate from `Selection.sources`. The normalized
config validators reject escaped or non-normalized paths from persisted data.
Historical run configs decode a missing list as empty. Plan schema 4 requires
regeneration of earlier manifests before any baseline runs; fingerprint schema
7 frames the ordered roots under field tag 9, preventing old-session reuse.
Users must start a new session and pay the baseline and mutant execution cost.

`WorkspaceHandler::with_import_roots` preserves the existing constructor and
copy lifecycle. The owned blocking worker-materialization task checks that explicit roots
exist as directories in the copied worker before publishing `WorkerCreated`.
Failed validation retains cleanup ownership through the existing pending-worker
path; command environment construction performs no filesystem reads. The error
identifies `--import-root` and suggests checking exclusions. Import roots do
not bypass copying policy or select additional targets. Environment order is
worker root, explicit roots, selected source roots, then rewritten inherited
PYTHONPATH, using the existing OS split/join and deduplication.

`tests/import_roots.rs` creates a network-free `venv --without-pip` and a
regular package exposed by a path-only `.pth`. It records actual module paths
and results for baseline and mutant, checks narrow file/line selection and
plan-to-verify inheritance, and retains an inherited-PYTHONPATH control. This
observes Python import behavior directly; Lean session/workspace oracles do not
prove how Python loaders resolve imports. The setting covers path-only `.pth`
regular packages when Python honors PYTHONPATH, not arbitrary editable finders,
custom loaders, or invocations with `-E`/`-I`.

### Cross-boundary contract replay

`tests/fixtures/boundary-contracts.json` connects six boundaries to exact tests.
Run `python3 tools/boundary_contracts.py strict --output /tmp/hoimin-boundaries`
from a worktree with its own Cargo target and prepared `.venv`. Each strict row
must execute and match; zero-test success, unavailable premises and external
deadlines cannot count as matches. The runner removes inherited minimal-case
filters so corpus coverage stays complete.

Run `python3 tools/boundary_contracts.py report --from-results
/tmp/hoimin-boundaries/strict.json --output /tmp/hoimin-boundaries` as one command
to retain failures and unexecuted native/preparation cases. Missing strict
results produce unexecuted rows, not inferred passes. CI runs this after the
existing Lean proof/sensitivity/freshness gate, then adapter checks, strict
replay, all-case report and a minimal result-validation witness. Native Linux
hard controls, Windows PID/fault cases and preparation cancellation remain
explicit report gaps. See [the worksheet](superpowers/specs/2026-09-14-issue-490-boundary-contracts-design.md).
## Python source encoding contract

Use `hoimin_core::decode_python_source` for source bytes that need Python text,
including symbol existence checks. Its `DecodedPythonSource` exposes `text()`,
`encoding()`, `utf8_to_raw()` and `raw_to_utf8()`. The two offset methods accept
only character boundaries. UTF-8/ASCII borrow source text; Latin-1 owns decoded
UTF-8 plus a sparse expansion index. This storage grows with source input and
is not bounded by `max_candidates`.

Candidate spans and source hashes always describe original bytes. Convert
Ruff's UTF-8 spans before constructing persisted candidates, then validate with
`CandidateValidationContext`. It owns decoded facts and shares the existing
Unicode-column index across candidates. The candidate strings are Unicode;
the source codec must encode original and replacement text. Keep replacement
representability checks before worker writes, and write raw prefix, encoded
replacement, raw suffix. Never apply decoded offsets directly to raw Latin-1
bytes or write `replacement.as_bytes()` for every codec.

The stable-ID schema remains 1: it already frames raw source hash/span and
Unicode replacement. Source records and fingerprint inputs remain raw bytes.
Run `cargo test -p hoimin-core --test source_encoding --test candidate_policy`
and `cargo test -p hoimin-cli --test source_encoding --test plan` for codec
changes. The CLI encoding fixture observes CPython worker bytes and values;
existing UTF-8 Lean/source-index proofs do not establish codec correspondence.
