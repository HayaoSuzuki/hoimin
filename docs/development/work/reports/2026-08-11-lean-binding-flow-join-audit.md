# Lean Binding-Flow Join Audit Report

Date: 2026-08-11

## Result

This audit found no same-premise mismatch in Hoimin's current propagation of
typing-import facts or builtin/exception name-resolution facts across the
covered control-flow and scope boundaries. No production analyzer behavior was
changed. The only Rust implementation additions are `#[cfg(test)]` projections
and a behavior-neutral collector initializer extraction.

The durable claim is deliberately narrower than “Lean proves the Rust AST
walker.” Lean proves laws of the explicit binding-flow model, owns the fixed
expectations and bounded corpus, and detects five deliberately broken semantic
families. Rust correspondence tests then compare those expectations with either
the public `hoimin plan` path or disclosed private test fixtures.

## Audited Surface

The model covers a flat knowledge meet, independently typed source and
destination gates, module / function / class frames, global and nonlocal
directives, categorized fallthrough / break / continue / terminate
exits, finally routing, and a fuel-indexed structured evaluator. Loops compute
a rank-bounded descending fixed point whose convergence is proved; try handlers
have an explicit conservative entry state;
match failure and irrefutable match are distinct; scoped statements resolve
their entry from the modeled lexical frame path rather than accepting an
unexplained environment.

The 25 fixed cases cover:

- agreeing and disagreeing `if` paths;
- zero-iteration, fallthrough, continue, and break loop paths;
- try/handler joins and falling-through or abrupt `finally`;
- unmatched, irrefutable, and guard-binding match paths;
- whole-block function locals and class/method lookup boundaries;
- global/nonlocal uncertainty, wildcard imports, and unconditional re-import;
- builtin and exception source/destination gates.

The audit excludes dynamic execution semantics, arbitrary Python programs,
comprehension evaluation order, loop `else`, detailed exception selection,
match-guard evaluation, scope write propagation, reflection beyond the modeled
conservative wildcard effect, mutation ranking, candidate limits, and a proof
that Ruff AST traversal implements `eval`.

## Lean Proofs

`BindingFlowProofs.lean` proves:

- commutativity, associativity, and idempotence of fact and environment meet;
- retained exact knowledge is present on every joined path;
- meet cannot invent exact knowledge;
- allowed candidates have exact source and destination facts;
- unrelated sibling isolation and method lookup skipping class scope;
- preservation of the incoming exit category through a falling-through
  finalizer;
- descending loop iteration and stabilization for a fixed back edge;
- the rank-bounded joined-transfer iterator always converges;
- every returned fixed-point result is stable under the joined transfer.

Every non-trivial public theorem has `maxHeartbeats 100000`. The new Lean files
contain no `sorry`, `admit`, custom `axiom`, or unlimited-heartbeat setting.

## Sensitivity

All five deliberately broken variants are distinguished by literal witnesses:

| Broken family | Detected |
| --- | --- |
| union in place of conservative meet | yes |
| method resolution through an intervening class | yes |
| finally losing the original exit category | yes |
| loop iteration replacing rather than meeting the head | yes |
| source-only candidate gating | yes |

The Rust fixture additionally removes continue edges from a test-only loop
fixed-point implementation. With the edge, `Sequence` becomes unknown at the
head; without it, `typing.Sequence` is incorrectly retained. A direct fixture
also confirms that a falling-through finalizer restores a `continue` exit to
the `continues` category.

## Correspondence Worksheet

| Mode | Cases | Observation path | Result |
| --- | ---: | --- | --- |
| `strict` | 18 | public `hoimin_cli::run_with_io` → `hoimin plan` → `PlanManifest` | all matched |
| `internal-fixture` | 4 | `#[cfg(test)]` categorized `KnownImports` snapshots | all matched |
| `model-only` | 3 | Lean scope/abrupt-flow premises without a same-premise public projection | all passed |
| `infrastructure-error` | 0 | reserved parser classification | no case classified |

Strict observations include presence, exact original text, replacement,
operator, symbol, and a candidate byte span contained by the unique literal
marker. The corpus states the source target (`builtin`) separately from the
destination target (`typing` or `builtin`); the Rust adapter rejects a corpus
whose operator/original/target tuple does not establish the same premise.
Case IDs form a closed scenario set, and each ID validates its mode, family,
and required Python control-flow/scope structure; a try scenario cannot be
silently replaced by a simple import fixture.
Nonzero CLI exits, stderr output, timeouts, malformed manifests,
missing/non-unique markers, and duplicate matching candidates are classified as
infrastructure errors rather than semantic mismatches.

For internal fixtures, Lean also owns normalized fallthrough, break, continue,
and terminate snapshots. The two `while` fixtures additionally carry the Lean
fixed-point loop-head snapshot, which Rust compares with its private
`KnownImports` projection. The previous Rust-owned blanket expectation was
removed.

Global and nonlocal cases are intentionally `model-only`: projecting the end of
the module suite would not observe the annotation inside the nested function,
so calling them internal-fixture cases would change the premise.

## Bounded Exploration

Depth 2 was retained:

- programs/states: 130;
- transitions: 612;
- state ceiling: 1024;
- fixed cases: 25.

Depths 3 and 4 were not run. A minimal `import Std` Lean process sampled about
656,576 KiB RSS, already above the agreed 512 MiB expansion-stop line. The
depth-2 run completed below the 768 MiB termination threshold, but its RSS also
exceeded 512 MiB, so expansion stopped without retrying at higher limits.

## Resource Ledger

The guard uses a 20-second deadline, samples root-plus-descendant RSS every
250 ms, terminates the process group at 768 MiB, and writes atomic JSON stats.
It refuses unsupported process-group platforms before spawning. If the root
exits while a descendant remains in its process group, the guard kills the
group and reports monitor error 126 rather than returning success.
After SIGTERM it polls the process group, escalates remaining members to
SIGKILL, and polls again; the regression descendant deliberately ignores
SIGTERM.
It is a userspace watcher, not a kernel hard limit: allocation can grow between
samples. This was observed once when the native executable build was detected
at 800,512 KiB (about 782 MiB), returned exit 125, and was terminated. The
limit was not raised; later builds were serialized and scoped to changed
targets before the final cached full-library check. Corpus commands use the
already built executable.

Fresh final evidence is reported as the highest 250 ms sample observed, not a
true peak. These are lower bounds; short-lived native executables can finish
before a representative sample.

| Command | Elapsed | Highest observed RSS sample | Result |
| --- | ---: | ---: | --- |
| theorem consumer | 2,424 ms | 575,040 KiB | child exit 0 |
| final changed-target build, `-Kjobs=1` | 5,142 ms | 782,736 KiB | child exit 0 |
| final full library build, `-Kjobs=1` | 2,169 ms | 644,016 KiB | child exit 0 |
| depth 2 stats | 290 ms | 1,712 KiB | child exit 0 |
| five sensitivity witnesses | 291 ms | 3,344 KiB | child exit 0 |
| 25 fixed cases | 836 ms | 32 KiB | child exit 0 |
| corpus freshness | 288 ms | 3,184 KiB | child exit 0 |
| abandoned native executable build | 2,171 ms | 800,512 KiB | RSS stop 125 |

The five Python guard tests separately confirm unsupported-platform preflight,
normal exit propagation, timeout exit 124, RSS exit 125, and cleanup/reporting
when a root exits while leaving a descendant alive.

## Verification

Fresh successful commands:

```text
python3 -m unittest tests.test_lean_resource_guard -v
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
uvx ruff check formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
uvx ruff format --check formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
git diff --check
```

All workspace tests completed with zero failures; benchmark and subprocess
fixture tests marked ignored by the existing suite remained ignored. Clippy
completed with warnings denied.

Each Lean command below was run separately through
`tools/lean_resource_guard.py` with `--timeout-seconds 20`,
`--rss-limit-mib 768`, and `--sample-ms 250`:

```text
lake env lean /tmp/hoimin-binding-flow-proof-consumer.lean
lake -Kjobs=1 build HoiminOracle.BindingFlowCases generate_binding_flow
lake -Kjobs=1 build
.lake/build/bin/generate_binding_flow --stats 2
.lake/build/bin/generate_binding_flow --sensitivity
.lake/build/bin/generate_binding_flow --cases
.lake/build/bin/generate_binding_flow --check corpus/binding-flow-joins.jsonl
```

## Outcome and Next Target

There was no confirmed implementation bug, specification ambiguity, or
infrastructure error in the retained correspondence set. One modeling/adaptor
classification issue was corrected before correspondence: nested global and
nonlocal observations cannot be represented by a module-suite exit snapshot,
so they remain model-only.

A future audit can add a marker-addressed private annotation-site projection for
global/nonlocal and comprehension internals. It should reuse the same guard and
must not increase structured depth unless Lean's baseline RSS falls below the
512 MiB expansion line.
