# Return constant evidence and limits

The original paper, [Will My Tests Tell Me If I Break This Code?, §3](https://arxiv.org/pdf/1611.07163),
uses extreme method transformations with complementary constants. Its Java results
motivate this mutation family; they do not establish effectiveness for Python or
for annotation-selected functions. This implementation keeps the operator opt-in.

Public saved-plan tests cover all three kinds. Calls without result assertions let
both mutants survive. Assertions about both Boolean outcomes, the computed integer,
and the constructed string kill both respective mutants. Candidate identities stay
unchanged across weak/strong commands; baselines pass, runs complete and source bytes
remain unchanged. Runtime traces independently show retained defaults/decorators and
docstrings, erased body effects, and the selected constant result.

The [trial](trial.py) copies local installed package sources into temporary projects,
uses authored checks and verifies only the top three candidates. [Observations](observations.json)
include commands, versions and source hashes. They are not upstream-suite benchmarks:

| Module (packaging 26.3) | Candidates | Selected outcomes |
| --- | ---: | --- |
| packaging.markers | 12 | 1 killed, 2 survived |
| packaging.version | 4 | 0 killed, 3 survived |
| packaging.specifiers | discovery failed | no mutation outcome |

The specifiers source triggers the existing debug assertion `duplicate unary-not
operator start` in `fact_index.rs`. The same invocation selecting only the preexisting
`function_body_erase` reproduces it before mutation collection. Both diagnostics are
retained separately; this is not a killed mutant, and this PR does not change that
index. The other baselines exited 0 and all selected trials completed. Limited command
coverage and unexplained survivors prevent any aggregate benefit or pseudo-tested claim.

## Formal correspondence

| Contract | Evidence | Scope |
| --- | --- | --- |
| Replacement returns chosen constant without body state effects | Generic Lean action theorem | Model-only |
| Two distinct constants; exact literal no-op excluded | Lean kinds, choices and theorems | Tagged literal model |
| Own return vs nested return, own vs eager-header suspension | Lean statements and lemmas | Syntax summaries |
| Eligibility and exact candidate pairs | 32 Lean cases, 23 candidates, public CLI | Strict finite correspondence |
| Prefix/docstring/suffix retention | Generic Lean string-span model, encoded public tests | Model proof plus bounded implementation observations |
| Python syntax and actual traces | CPython 3.14 compile and runtime tests | Not a parser or type proof |

The model does not prove annotation resolver correctness, Python literal decoding,
actual runtime types, parser correctness, coverage, or pseudo-tested classification.
Python 3.14 type-parameter shadows are tested in Rust/public CLI in addition to the
portable generated sources. All 32 sources and 23 generated mutants compile.

Fourteen broken controls detect retained body effects, wrong constants, literal no-op,
bool/int conflation, nested-return counting, nested-body suspension counting, ignored
eager headers, lost docstring, annotation, binding, async, dunder, missing own return
and suspension. No `sorry`, `axiom` or `native_decide`. Fixed fixtures have search depth
zero. Local commands use one process and a 20-second/2048-MiB guard; CI keeps its
existing 30-second guard. The RED retained a state increment and Lean rejected it
(3.273 s / 634688 KiB) before GREEN.

Model: 2.825 s / 738800 KiB; Main: 3.059 s / 687328 KiB; generate/link: 2.165 s /
768240 KiB; freshness: 0.483 s / 57072 KiB; sensitivity: 0.254 s / 57024 KiB.
From `formal/HoiminOracle`, reproduce with
`python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 100 --stats /tmp/return-stats.json -- lake exe generate_function_return_constant -- --check corpus/function-return-constant.jsonl`;
replace `--check ...` with `--sensitivity` for the controls. The worksheet is embedded
in `HoiminOracle/FunctionReturnConstantModel.lean`.
