# Inheritance mutation evaluation: issue 1123

The [product follow-up](implementation.md) implements `base_order_shuffle`,
`override_method_delete` and `super_call_move` as default runtime operators with
individual opt-out. The catalog has 104 IDs / 85 defaults. The research counts
and receipts below describe the earlier phase and remain historical evidence.

## Decision

Evaluate base main `312b72d3e06e44145bbac28e998c97a8ca4f587d` on 2026-10-28.
Recommend **limited adoption of two operators**, `override_method_delete` and
`super_call_move`, under the static restrictions below. Do not add a separate
`super_call_delete`: ordinary standalone delegation deletion already exists.
The [base-order follow-up](base-order.md) evaluates an additional
`base_order_shuffle` prototype on six multiple-inheritance examples. It finds
additional assertion gaps, equivalent controls and an inconsistent-MRO error;
it does not extend the single-inheritance eligibility proposal below.
The original research phase recorded the decision and executable evidence without
changing its 101-ID catalog / 82 defaults.

| Proposal | Decision | Evidence and qualification |
| --- | --- | --- |
| Delete an overriding method (IOD) | Limited adoption | Specialized provider dispatch exposes a missing behavior assertion absent from every generated default candidate in that fixture. Restrict to ordinary same-module single inheritance and compatible method signatures; exclude constructors and descriptors. |
| Move a super call (IOP) | Limited adoption | Callback event ordering exposes a missing order assertion absent from every generated default candidate in that fixture. Direct list append ordering already overlaps existing operators. Restrict to straight-line same-method standalone calls; runtime dependency failures remain possible. |
| Delete a super call (SCD) | No additional operator | Both deletion fixtures produce a byte-identical mutant through default `statement_delete`. Existing guards deliberately exclude binding/suspension changes. |

This is evidence of additional behavioral coverage in two small examples, not
an estimate of prevalence or a claim of global non-overlap. An existing useful
candidate for the same requirement and a new useful candidate do not count as
two independent missing tests. No requirement to reject all inheritance
changes or to maximize mutation scores is introduced.

## Reproducible comparison

[experiment.py](experiment.py) originally inspected nine independently written ASCII
examples without importing them to generate candidates. The tiny AST prototype
only knows these fixtures; it is **not** an implementation of the proposed
eligibility guards. It deliberately includes two risk counterexamples.
The saved original receipts and the table below cover those nine examples.
The extended runner includes all fifteen by default; `--base-order-only`
selects the six follow-up examples and `--seed` controls their permutations.

For each fixture, require the original to pass both weak and stronger checks.
Generate (1) the nearest related pool using `statement_delete`,
`function_body_erase`, `function_body_return_constant`, `method_call_remove`,
(2) all current default candidates, and (3) one prototype candidate. Execute up
to 64 candidates per pool with fresh processes and identical checks. All pools
fit that budget: **zero unexecuted candidates**. The nearest pool explicitly
includes opt-in operators; it is not limited to defaults. The full catalog's
other opt-in operators are not exhaustively compared.

Each table cell is **pool / useful leads / passes both / errors**. A useful
lead survives the weak check and fails a strong assertion based on the stated
API requirement. An error in either check is counted separately and never
credited as a useful lead. Remaining candidates already fail a weak assertion.
Exact replacements, source, requirements, check text, exception types and
timings are in the JSON.

| Fixture | Nearest pool | Defaults | Prototype |
| --- | --- | --- | --- |
| override_dispatch | 0 / 0 / 0 / 0 | 4 / 0 / 3 / 1 | 1 / 1 / 0 / 0 |
| override_equivalent | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 1 / 0 / 1 / 0 |
| constructor_interface_risk | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 1 / 0 / 0 / 1 |
| super_order | 6 / 0 / 0 / 0 | 13 / 2 / 4 / 0 | 1 / 1 / 0 / 0 |
| super_order_callback | 6 / 0 / 0 / 0 | 7 / 0 / 0 / 0 | 1 / 1 / 0 / 0 |
| super_commuting_control | 5 / 1 / 0 / 0 | 7 / 3 / 0 / 0 | 1 / 0 / 1 / 0 |
| super_data_dependency_risk | 4 / 2 / 0 / 2 | 5 / 2 / 2 / 1 | 1 / 0 / 0 / 1 |
| super_delete_overlap | 4 / 4 / 0 / 0 | 5 / 3 / 2 / 0 | 1 / 1 / 0 / 0 |
| super_delete_no_observation | 2 / 0 / 2 / 0 | 1 / 0 / 1 / 0 | 1 / 0 / 1 / 0 |

Both [CPython 3.14.9](results-python314.json) and
[CPython 3.15.0b1](results-python315.json) produce these verdict counts and the
same descriptors. Each interpreter executes 18 original checks and 156 mutant
checks (78 candidates including repeated pools). The beta is additional
evidence, not a stable Python release claim. Counts are fixture-specific;
passing both checks is observational equivalence for that domain, not a proof.

The positive dispatch fixture supplies equal provider results in the weak check
and different results in the strong check. Deleting the child definition falls
back to the parent provider, while body erasure/constant returns cannot express
that fallback. The source replacement is not present in either existing pool.

The positive callback fixture requires the sink to receive `before`, `parent`,
`after` exactly once. Weak checks observe membership/multiplicity; the stronger
check observes order. Existing deletions lose events and already fail weak
checks. Moving delegation retains events but violates the order requirement.
In the direct-list fixture, `collection_append_insert` also exposes the missing
order assertion; retaining this overlap example prevents overstating novelty.

The constructor mutant raises `TypeError` by losing its accepted argument. The
data-dependency mutant raises `AttributeError` by reading state before parent
initialization. Neither is presented as an assertion gap. Excluding constructors
addresses the first; a straight-line syntax restriction alone cannot eliminate
the second. The commuting-set and identical-forwarder controls survive both
checks. Agents must assess such candidates against an actual contract.

## Proposed static eligibility and exclusions

These are implementation requirements, **not tested product guarantees**. The
fixed-corpus prototype does not reject arbitrary untrusted source.

### Shared inheritance boundary

Start with one top-level child and one earlier top-level parent in the same
module. The parent has no explicit bases; the child has exactly that parent
Name as its sole base, without class keywords, decorators or type parameters.
Reject nested/conditional classes, imported/attribute/call bases, multiple
inheritance, explicit/dynamic metaclasses, class decorators and unknown dynamic
namespace changes. No runtime MRO discovery or target import.

Build one shared, ordered module binding/class index, only when a selected new
operator needs it. Follow proven source bindings rather than matching the
spelling of a class name. Reject redefinitions, aliases/escapes or writes that
can alter parent or child method lookup, including `setattr`, `exec`, wildcard
imports and `__bases__` changes; do not add a runtime compatibility fallback.
An arbitrary trusted local program can monkey-patch externally; this static
model is intentionally not a proof against all possible external mutation.

Initially accept ordinary undecorated instance methods, `self` first, with no
async, generators, nested definitions or descriptor/property/staticmethod/
classmethod behavior. Reject special methods including `__init__`, `__new__`,
`__getattribute__`, and `__init_subclass__`. Parent and child must each have one
unambiguous same-name ordinary method; no assignment may replace that binding.
Reject defaults and annotations with evaluation side effects rather than
silently deleting definition-time behavior. Start with matching parameter
kinds/names and no defaults, annotations or variadics, and document the
restriction explicitly.

### override_method_delete

Replace the **entire child method definition with `pass`**, retaining a valid
class suite and leaving the parent definition intact. Deleting only the body
keeps overriding with an empty function and is a different mutation. Do not
infer usefulness from the mere presence of an override; identical behavior can
be equivalent. Emit at most one candidate per eligible override. Direct
base-method calls, exported aliases of the method, duplicate definitions and
property setters require explicit exclusion rather than optimistic resolution.

### super_call_move

Require builtin zero-argument `super()` with proven unshadowed identity and a
single standalone `super().<same_method>(...)` in the direct method body. No
assigned/returned delegation, explicit `super(Type, self)`, other-method calls,
dynamic attribute selection, multiple calls, starred expansion, walrus,
await/yield or control-transfer constructs in the moved region.

Move the call from the first executable position to the last, or otherwise to
the first, with at least one intervening executable statement. Exclude docstring
movement and retain the first docstring unchanged. Restrict the region to simple
expression statements initially; do not move calls across return, raise,
try/finally, with, loops, nested scopes or branch boundaries. Emit one candidate
per method rather than all possible permutations. Side-effect dependency errors
are still possible and must remain distinct from useful kills in agent review.

### Deletion overlap

`rust.rs` already adds `statement_delete` for standalone calls, guarded by
`deletion::can_remove`. The guard rejects named assignments, await, yield and
yield-from within an expression. Ordinary `super().run(events)` is eligible;
its replacement `pass` is byte-identical in both measured deletion examples.
Do not weaken that guard or create another operator ID for this spelling.
Assigned and returned super calls lie outside this duplication claim.

## Adoption implementation checks

Two IDs are the smallest useful addition supported here. On implementation,
enable accepted IDs by default with individual `--exclude-operators`, following
the existing opt-out policy. Reuse the existing candidate collector, selection,
payload limits, cancellation and ranking categories; no new schema, dependency,
configuration file, runtime MRO cache or persistent analysis state is warranted.

Required coverage before product adoption:

- Analyzer positive/negative cases for every boundary above, including binding
  order, module/local `super` shadowing, method decorators, class rebinding,
  signatures, multi-inheritance, async/generator methods and dynamic writes.
- Actual CPython compilation and execution of original/mutant source on supported
  versions, with weak survival, strong assertion failure, equivalent controls
  and explicit runtime-error controls separated.
- Token-aware source edits preserving comments, multiline methods/calls,
  parentheses, Unicode, encoded source, CRLF, lone-CR and one-line suites;
  compile transformed sources rather than using AST unparse as product output.
- `--file`, `--symbol Child.run` and line-range selection against the replaced
  span, with unchanged base/neighboring definitions, stable repeated plans,
  per-ID opt-out, candidate/payload caps, profile exclusions and cancellation.
- Public saved-plan/verify checks with fresh baseline, honest counts, no original
  source modification and no target imports during planning. A fixture whose
  import raises should still support static candidate generation.
- Traversal/allocation counters on large class inventories: one bounded index,
  no per-method whole-module scan, no all-permutation source copies. Measure
  plan latency and peak memory against the recorded base before claiming a
  performance impact; current prototype timings do not estimate that cost.

## Resource and evidence limits

The runner reuses the prior fixed experiment's subprocess, plan, edit and
check helpers. Each check has a five-second timeout, plan a 45-second outer
deadline. Plans use one job, 1,000 candidate and 100 mutant limits, 10-second
baseline/mutant timeouts, 60-second total timeout, 8-GiB workspace budget and
10-GiB free-space reserve. Assert free space before each experiment. No full
project benchmark or mutation campaign is performed.

The complete experiment took 3.32 seconds on CPython 3.14.9 and 3.21 seconds
on 3.15.0b1 on the recorded macOS host. Per-fixture public planning took roughly
8–16 milliseconds across both existing pools; these nine tiny examples do not
predict costs for large inheritance inventories. Final temporary-file totals
were 3,728 and 3,830 bytes respectively, with both temporary roots removed.

Direct fixed-source checks run fresh interpreters outside the product worker
manager, under a per-process timeout; this is not evidence of sandboxing,
hard memory containment or descendant process cleanup. These fixtures create
no subprocesses themselves. `TemporaryDirectory` owns cleanup even on an
exception. JSON records the final removed status, temporary file size, source
revision, binary/runner/helper SHA-256, interpreter, elapsed time and every
executed candidate. Measured CLI planning and a tiny Python AST prototype are
different workloads and cannot support a speed comparison. No new Lean proof
or universal equivalence claim is made.

## Reproduce and references

Build from the recorded base main; running a later binary can change the pools.
From the repository root:

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 \
  cargo build --locked -p hoimin-cli --bin hoimin
.venv/bin/python -B docs/superpowers/reports/issue-1123/experiment.py \
  --hoimin target/debug/hoimin --output /tmp/hoimin-1123-replay.json
```

The runner is a disposable research tool, not a supported user command. The
original JSON receipts remain historical evidence; do not overwrite them when
changing the corpus or binary. [Self-review](self-review.md) records verification.

- [Issue 1123](https://github.com/HayaoSuzuki/hoimin/issues/1123).
- [Product principles](../../../product-principles.md).
- [MutPy inheritance operators](https://github.com/mutpy/mutpy/blob/5c8b3ca0d365083a4da8333f7fce8783114371fa/mutpy/operators/inheritance.py):
  IOD replaces a definition with `Pass`; IOP moves first delegation to the end
  and otherwise to the start; SCD replaces delegation with `Pass`. MutPy's
  override detection uses an imported module and `type.mro`, which is not
  suitable for hoimin's static planner and is not copied.
- [Existing collector](../../../../crates/hoimin-python/src/analyzer/rust.rs)
  and [deletion guard](../../../../crates/hoimin-python/src/analyzer/rust/deletion.rs).
