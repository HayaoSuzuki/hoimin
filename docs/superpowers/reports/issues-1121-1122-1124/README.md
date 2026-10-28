# Slice, division and survivor-directed selection research

## Research decisions

Research performed on 2026-10-28 against `7c2a7916` on macOS arm64.
The subsequent [implementation](implementation.md) adds the accepted operators
on the same branch after the maintainer requested implementation. The comparison
tables below retain the original research revision and are not new measurements.
This completes the bounded evaluations requested by
[#1121](https://github.com/HayaoSuzuki/hoimin/issues/1121),
[#1122](https://github.com/HayaoSuzuki/hoimin/issues/1122) and
[#1124](https://github.com/HayaoSuzuki/hoimin/issues/1124).

| Issue | Decision | Evidence boundary |
| --- | --- | --- |
| 1121 | Limited adoption as a follow-up candidate: two narrowly scoped slice operators, enabled by default when implemented | Variable-bound examples expose missing range checks; literal cases overlap existing neighbors; omission also produces equivalent candidates |
| 1122 | Do not add a product scheduling policy now; use existing line diversity and agent-selected batches | Line suppression misses a second requirement on the same line; exact-span suppression reduces duplication less; existing CLI can express both externally |
| 1124 | Limited adoption as a follow-up candidate: one `/` / `//` exchange operator, enabled by default when implemented | Fraction checks expose gaps missed by existing arithmetic mutants; conversion/domain equivalence and unsupported overloads limit applicability |

"Limited adoption" means the evidence warrants a minimal, restricted implementation
follow-up. The research-only commit `1efe26f5` added no product operators,
selectors, defaults, saved formats or scheduler behavior. No broad effectiveness claim follows from
these independently constructed examples. Per the maintainer's clarification,
adopted operators should be enabled by default and removable using
`--exclude-operators`; restrictions apply to syntax and semantics, not to an
opt-in flag. A real-project trial remains necessary to establish broader value.

## Reproduce and inspect

Read the [fixed protocol](methodology.md), then run the standard-library-only
[experiment adapter](experiment.py) from the repository root:

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked -p hoimin-cli
python3 docs/superpowers/reports/issues-1121-1122-1124/experiment.py \
  --hoimin target/debug/hoimin --output /tmp/hoimin-research.json
```

The recorded runs use CPython 3.14.9 and 3.15.0b1, the latter explicitly a beta.
Results are in [Python 3.14 evidence](results-python314.json) and
[Python 3.15 beta evidence](results-python315.json). Each records the actual
Python/platform, full base revision, binary SHA-256, candidate edits, checks,
verdicts, policy traces, real CLI counts, disk cleanup and elapsed times.
The adapter runs only this fixed ASCII corpus; it is not a public CLI or an
arbitrary-project test runner. The build verifies the current Rust source;
candidate generation for proposed operators is an independent Python AST/token
prototype, not a tested Ruff implementation.

Use a binary built from the recorded research revision to reproduce its exact
candidate pools. Replaying with the implementation revision changes the default
full-profile control because the three new operators are now enabled. The
recorded files are preserved, rather than overwritten with that different run.

Every original passes both weak and strengthened checks. A useful lead requires
a weak survivor and an assertion failure under a separately stated requirement.
Runtime exceptions and bounded-domain equivalence are reported separately.
For each example and family, the budget ceiling is two executed mutants; empty
pools use no execution budget. Baseline checks and full direct-probe census are
research setup costs, not counted as mutations or hidden in the policy budget.

## Issue 1121: slice boundaries

`collect_subscript` and `collect_slice_neighbors` in
[the current analyzer](../../../../crates/hoimin-python/src/analyzer/rust.rs)
restrict numeric-neighbor generation to load-context, non-annotation subscripts.
`decimal_literal_neighbors` covers plain decimal and unary-negative integer
bounds, including step with zero excluded.
[#471](https://github.com/HayaoSuzuki/hoimin/issues/471) addressed negative
neighbors; it does not add variable-bound omission or movement. Its historical
empty-candidate observation is superseded by current negative-bound tests in
[rust_tests.rs](../../../../crates/hoimin-python/src/analyzer/rust_tests.rs).

The comparison includes the direct incumbent (`structure_slice_neighbor`) and
the existing full default profile. On these five examples they yield the same
pools and prefix verdicts. "Useful" below counts weak-surviving candidates
killed by the independent stronger check, not distinct requirements.

| Example | Existing pool / executed / useful | Proposed pool / executed / useful | Interpretation |
| --- | --- | --- | --- |
| `xs[a:b]` | 0 / 0 / 0 | 2 / 2 / 2 | Missing lower/upper exclusion checks; one window requirement |
| `xs[:n]` | 0 / 0 / 0 | 1 / 1 / 1 | Prefix versus remainder is observable on nonempty input |
| `xs[n:]` | 0 / 0 / 0 | 1 / 1 / 1 | Suffix versus prefix is observable on nonempty input |
| `xs[1:3]` | 4 / 2 / 2 | 2 / 2 / 2 | Same window check kills both families; no extra useful requirement |
| `xs[0:99]`, list length at most 3 | 3 / 2 / 1 | 2 / 2 / 0 | Both proposed omissions preserve the allowed bounded-domain behavior |

The proposed family has eight candidates: six useful leads for four fixture
requirements, and two bounded-domain controls. Only three of those requirements
are additional to the existing family in this corpus. Do not count the two
window omissions as two independent test defects.

Omitting `0` as a lower bound is equivalent for these list inputs; dropping `99`
is equivalent only under the stated length restriction. A longer list can
distinguish the latter. Neither observation proves equivalence for custom
`__getitem__` implementations. Candidate-count growth matters even without test
execution, so whole-range replacement is not included.

### Minimal follow-up and conditions

- At most two identities: bound omission (up to two candidates for a
  bounded slice) and bound movement (one for a one-sided slice). No extra
  all-range mutation. Include adopted identities in the default runtime set and
  support exclusion through the existing `--exclude-operators` mechanism.
- Initially retain load-context, one-dimensional slices without a step value,
  with simple integer/name bounds. Exclude annotation, assignment/deletion
  targets, call/attribute/compound bounds and multidimensional indexing.
- Omission removes a bound's evaluation; movement preserves a single bound
  evaluation but changes the slice object. Even a name can become unbound, and
  custom slicing may observe the omitted value or execute different code.
  Do not claim type inference or universal side-effect equivalence. Target code
  must not be imported to generate candidates. No builtin symbol lookup is
  necessary for the syntax itself; bindings and custom receiver behavior remain
  runtime concerns.
- Production validation must cover negative/zero bounds, empty and overlong
  sequences, adjacent/commented/multiline slices, UTF-8 byte ranges, annotations,
  excluded contexts, source reparsing on supported Python versions, selector and
  default membership, max-candidate admission and cancellation. Current prototype
  exclusions and negative controls establish only the narrower recorded corpus.

## Issue 1124: true/floor division exchange

Current token replacements map `*` to `/`, `/` to `*`, `//` to `%`, and `%` to
`//`. Exchanging `/` and `//` probes a different requirement: retaining fractions
versus flooring, especially for negative inputs.

| Example | Existing direct family | Proposed exchange | Independent conclusion |
| --- | --- | --- | --- |
| Real quotient | One candidate, killed by `12 / 3 == 4` already | One weak survivor, killed by fractional checks | Weak divisible-only tests miss lost fractions |
| Floor quotient | One candidate, killed by `12 // 3 == 4` already | One weak survivor, killed by fractional checks | Positive and negative flooring need observation |
| `int(a / b)` for small nonnegative integers | Arithmetic candidate already killed | Survives both check sets | Under this domain, floor before conversion preserves the requirement |
| Input supporting only `__truediv__` | `*` raises `TypeError` | `//` raises `TypeError` | Protocol incompatibility, not a missing assertion |

The full-profile control agrees in the real/floor and overload cases. In the
conversion example it has an additional `conversion_call_remove` candidate,
which survives the weak check and fails the stronger check. That is a separate
existing way to probe integer-result behavior; it does not make the proposed
exchange useful for this permitted domain.

There are four proposed candidates: two useful leads for two requirements, one
domain-equivalent control and one runtime-error control. No syntax/startup
failure is counted as a useful behavioral kill. Dedicated negative-only checks
also pass on each original and fail by assertion on its exchanged mutant;
the first positive assertion does not stand in for negative-input execution.

### Minimal follow-up and conditions

One binary operator identity should exchange both directions by changing
only the operator token. Exclude augmented assignment, operator-function calls
and unrelated arithmetic in the initial implementation. No target import,
numeric coercion, new dependency or name-based builtin recognition is needed.

Left and right operand evaluation must remain once each, left to right. The
probe checks this explicitly; the result type changes from float to int for
integer operands and from the applicable overloaded method to its counterpart
for user-defined operands. The tool cannot infer support for both protocols
from syntax. Division by zero, unsupported types and other runtime exceptions
must remain execution outcomes, never be presented as justified test fixes.

Before implementation acceptance, test negative/fractional/zero-denominator
values, int/float and supported overloads, parentheses and comments between
operands, multiline and UTF-8 source spans, reparsing under supported Python,
selector identity/default inclusion and explicit exclusion, candidate caps and cancellation. Decimal,
Fraction and third-party numeric types were not surveyed in this research.

## Issue 1122: stop after a survivor

The fixed pool has 12 candidates over three source start lines. It is generated
by current `plan`, ordered by the public `verify --selection-policy line-diverse
--dry-run` preview, and evaluated with the same source and weak command. The
first line returns `(n + 1, n * 2)`; increment and doubling have independent
requirements. Weak checks leave increment and nonzero doubling unchecked.

Line grouping uses `(file, start line)`; this one-file fixture records the line.
Exact-span grouping uses `(file, byte start, byte length)`; the file is again
constant. Starting from the same line-diverse order, only an actual `survived`
blocks later members. Other statuses are tested not to suppress a group.

| Budget ceiling | Existing line-diverse: runs / useful requirements | Stop line | Stop exact span |
| --- | --- | --- | --- |
| 4 | 4 / increment | 4 / increment | 4 / increment |
| 8 | 8 / increment | 7 / increment | 8 / increment |
| 12 | 12 / increment, doubling | 7 / increment | 10 / increment, doubling |

The smaller-budget rows are fixed-pool trace-prefix comparisons. At ceiling 12,
all policies were replayed through the actual CLI. Adaptive replays pass exact
candidate IDs one at a time, using the completed result to decide what to defer;
they do not consult the exhaustive research oracle. ID order and every verdict
must match the trace. All real `verify` runs complete successfully, with zero
`not_run` **among their selected IDs**. This does not mean the deferred pool was
verified: every trace lists selected, deferred and all unexecuted IDs separately.

Line suppression saves five mutant executions but misses doubling. Exact-span
suppression saves two while finding the same two requirements in this fixture.
Neither improves the number of useful requirements under the tested ceilings.
Exact spans also fail to group many semantically similar operators. A line is
too coarse; a token span is too narrow to identify a behavioral requirement.

The measured real-CLI costs include fresh baseline and preparation on each call:

| Python | Policy | Mutants | Fresh baselines / verify calls | Total CLI elapsed (s) | Peak CLI-owned bytes |
| --- | --- | --- | --- | --- | --- |
| 3.14.9 | line_diverse | 12 | 1 | 0.354 | 19403 |
| 3.14.9 | stop_line | 7 | 7 | 0.717 | 17775 |
| 3.14.9 | stop_span | 10 | 10 | 0.990 | 17775 |
| 3.15.0b1 | line_diverse | 12 | 1 | 0.346 | 19607 |
| 3.15.0b1 | stop_line | 7 | 7 | 0.720 | 17979 |
| 3.15.0b1 | stop_span | 10 | 10 | 0.980 | 17979 |

These single-run timings are observations of tiny examples, not speed estimates
for real projects. The extra baselines explain why executing fewer mutants is
not equivalent to taking less time. A future integrated scheduler could avoid
that overhead, but this corpus supplies no value sufficient to justify its
saved-plan, report, resume, progress and in-flight concurrency contracts.

Existing exact-ID selection can implement an external agent's selective
deferral today. Preserve a separate deferral ledger and produce normal reports
only for the actual chosen IDs. Review the first survivor's requirement before
suppressing others; do not label an entire source line checked. No public
`not_run`, `complete`, exit-code or progress semantics change in this branch.
Parallel scheduling and resume effects were analyzed as future maintenance
costs, not implemented or empirically evaluated.

## Limits and validation

This is a small, independently authored corpus with explicit requirements, not
a real-project sample or copied book code. No efficacy statistics, general
equivalence proof, new sandbox or hard macOS memory guarantee is asserted.
The CPU and disk costs of prototype generation differ from a future Rust
implementation. Only one worker and trusted local examples were executed.

Both Python runs enforce a 10 GiB free reserve and remove their exact temporary
directory on completion. Real CLI reports retain configured limits, peak owned
disk bytes and cleanup status. No retained worker path or disk/resource stop
occurred. The local toolchain build used two jobs, no incremental compilation
and no debug information. Host free space stayed above 160 GiB during execution.

See [self-review.md](self-review.md) for five reviews per step, corrections and
the final check results. Research evidence stays in this branch together with
the [OKF decision](../../../knowledge/mutation-research-1121-1122-1124.md).
