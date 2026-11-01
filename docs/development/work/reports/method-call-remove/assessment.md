# Method call removal: evidence and limits

The operator follows the shape of RemMetCall in PyTation §2.5.3, with a restricted
static eligibility rule. The paper's dynamic classification and aggregate results
are not evidence for this implementation's precision or effectiveness.

The public saved-plan test calls `clean(' x ')`. Checking only the returned type
leaves the `(text)` mutant alive; checking the trimmed value kills the same candidate.
This demonstrates one observable result-assertion gap, not general effectiveness.

The reproducible [trial](trial.py) copies installed package sources to a temporary
project and runs authored checks, not the projects' upstream suites. The [observations](observations.json)
record package versions, source hashes, commands, candidate counts and selected outcomes.
`packaging` 26.3 generated four candidates: the top three survived. `iniconfig` 2.3.0
generated two candidates: both survived. Both baselines exited 0 and runs completed.
These checks do not execute every mutated path; survival alone does not establish a
missing assertion or pseudo-tested code. Keep the operator opt-in. Reassess using
representative suites and manual classification before changing defaults.

The initial iniconfig probe copied only its `__init__.py` into a standalone module;
its relative import failed at baseline. The corrected trial copies its package tree
and imports the real package name. That infrastructure failure is excluded from the
mutation observations.

## Formal correspondence

| Contract | Evidence | Boundary |
| --- | --- | --- |
| Receiver result/state/exception is retained after one evaluation | Generic Lean `receiver_once`, `receiver_exception` | Abstract action model only |
| Lookup and invocation are removed | Broken retained-lookup/call controls | Deliberate mutation, not equivalence |
| Eligibility and exact original/replacement pairs | 23 Lean-generated cases through public `plan` | Six positive, 17 negative cases |
| Syntax and precedence | CPython 3.14 compiles every corpus source/mutant plus explicit operator tests | No general parser proof |
| Evaluation side effects | Public Python trace: receiver, lookup, call -> receiver | Authored runtime fixture |

Lean does not prove bound-method identity, result-type preservation, Python parsing,
or the whole Rust implementation. Nine sensitivity families detect duplicated
receiver, retained lookup, retained call, swallowed exception, missing parentheses,
bare callee, nonempty arguments, excluded context and unsafe receiver. No `sorry`,
`axiom` or `native_decide` is used. The model's correspondence worksheet records the
abstraction at `formal/HoiminOracle/HoiminOracle/MethodCallRemoveModel.lean`.

All local Lean commands used one process and a 20-second/2048-MiB guard (search depth
zero, fixed fixtures). RED duplicated receiver state and was rejected in 3.094 s.
Model: 3.414 s / 701872 KiB; Main: 3.414 s / 658176 KiB; generator: 2.282 s /
792480 KiB; freshness: 0.609 s / 83296 KiB; sensitivity: 0.256 s / 56944 KiB.
The preexisting CI guard remains 30 seconds. Reproduce with
`python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 100 --stats /tmp/method-stats.json -- lake exe generate_method_call_remove -- --check corpus/method-call-remove.jsonl`
from `formal/HoiminOracle`; replace `--check ...` with `--sensitivity` for controls.
