# Task 3 independent review

Scope: BASE 17135904 through HEAD 68ea9956d63c13033a4f4a4153099e3e08cebd89, production arithmetic proofs only. Reviewed the Task 3 brief, approved eight mathematical clauses, implementer report, supplied commit/stat/full-diff package, and focused proof evidence. No git commands, source edits, index/HEAD changes, proof reruns, or broad test reruns were performed.

## Spec Compliance verdict

PASS for Task 3. All eight mathematical clauses are represented by contracts on the actual production functions and supported by the supplied fresh-generation proof evidence on all three required Rust targets. No public input restriction, extra projection provenance invariant, owned trust hole, or verification-only runtime arithmetic was found.

| Clause | Review result and location |
| --- | --- |
| 1: mathematical ceiling and safety | `crates/hoimin-budget-math/src/projection.rs:54` states ceiling over mathematical integers; line 69 keeps the overflow-safe quotient/remainder production body. Generated Coma includes division and addition safety obligations over unrestricted UInt64 selected and positive jobs. |
| 2: automatic timeout | `crates/hoimin-budget-math/src/duration.rs:108` defines the required saturated expression and line 113 contracts the common production body. Both multiplication and addition overflow paths remain. Focused caller inspection confirms `machine.rs:980` uses the same imported function for execution, while projection calls it at `projection.rs:71`. |
| 3: Fixed/Auto selection | `projection.rs:55` contracts both variants, with positive Fixed input preserved exactly and Auto delegated to the common function. |
| 4: saturated product | `projection.rs:88` specifies mathematical min(Dmax, duration × factor); lines 90–95 handle both u128 product overflow and representable products beyond Dmax. Generated Coma retains both branches and the upstream checked-multiply postconditions. |
| 5: exact reconstruction | `projection.rs:100` restricts only the private reconstruction helper; its caller establishes the Dmax bound. The quotient fits u64, the remainder is below 10^9 and fits u32, and line 101 requires exact integer equality. Generated Coma includes the Duration::new precondition and exact result obligation. |
| 6: strict shortfall | `projection.rs:42` specifies strict greater-than, preserving false at equality. Generated Coma calls the strict comparison and has only constituent Duration type invariants. |
| 7: preserved fields | `projection.rs:49` begins five separate preservation conditions; the common body copies those inputs and the generated postcondition retains all five. |
| 8: positive type and handmade projections | `duration.rs:40` defines positivity; lines 49 and 62 establish constructor/accessor meaning. Constructor Coma proves positivity for Some and exact zero for None. `TopBudgetProjection` has no owned Invariant; its generated invariant contains only its Duration fields, so manually constructed public values remain covered. |

## Strengths

- Proof configuration affects annotations, imports, logical models and dependency activation. Production value layouts and runtime bodies remain common. The private reconstruction refactor is small, its mathematical precondition is explicit, and it preserves the previously guarded conversion's behavior.
- The authorized common handwritten equality implementations preserve all fields and enum variants (`duration.rs:12`, `duration.rs:89`, `projection.rs:26`). Structural DeepModel derives provide independent specifications for enum/projection equality; the extra omitted-field mutation checks that guard.
- `crates/hoimin-budget-math/src/lib.rs:2` denies owned trusted code. Source and generated obligations show no owned external specification or assumption replacing the implementation. Standard-library contracts and the compiler/provers remain the declared trust boundary.
- `formal/creusot/obligations.json:1` gives Task 4 a substantive scope contract: eight-clause mapping, function attachment/anchors, complete precondition/postcondition sets, logical definitions, structural models, prohibition of a projection invariant, generated contract markers, all 59 path-qualified goals and target strategy roots. It expressly requires checking source semantics and all generated goals rather than accepting a goal count alone.
- `formal/creusot/README.md:145` and the durable review notes accurately distinguish ARM64 development-host measurements from the still-pending native x86_64 acceptance gate, and crate-cache cold measurements from environment-installation cold measurements.

## Evidence checked

- Independently matched all seven hashes in `task-3-evidence/final/final-source.sha256` against the current worktree: root Cargo manifest/lock, crate manifest, three production source files, and root Why3find configuration. Root and pinned formal Why3find configurations are byte-identical.
- Inspected `three-targets.sh`: three sequential targets, explicit target generation, package clean and generated-output removal before each round, actual workspace selected-project preflight, first proof with --no-cache, fresh warm generation followed by target-specific --replay --no-cache, and source-hash verification after each proof.
- Checked all six evidence rounds: each generation log reports checking the actual crate, the requested target and exit 0; each has 25 Coma files, all required generated markers, a 59-goal strategy inventory, and a proof log ending in Proved (25 files) with exit 0. Warm logs contain --replay and all proof logs contain --no-cache.
- All 75 committed strategy files match the warm evidence byte-for-byte. Parsed every strategy addition from the supplied full diff; all use pinned z3@5.1.0 goal strategies, and the additions match the current files.
- Inspected actual generated constructor, reconstruction, saturation, projection and shortfall Coma, including the public struct invariant and the private helper call precondition. This corroborates the source-to-obligation handoff beyond mere named markers.
- Inspected sensitivity.py and its results: each case changes one actual production expression, requires successful generation and the specific unproved goal, restores the file after the case, and has unconditional final restoration. The four required mutations plus structural equality report generation 0/proof 1. The controller independently inspected all five underlying failure logs; final passing source hashes match restored host source.
- Accepted the controller's supplied inspection of the full stable suite summary (2916 passed, 0 failed, 33 ignored) and six proof logs. The implementer records final Clippy/format/normal-hook success. No unanswered arithmetic doubt justified repeating those expensive runs.

## Critical findings

None.

## Important findings

None.

## Minor findings

None.

## Cannot Verify / scope limits

No unsupported claim blocks Task 3 acceptance. This review does not establish native Linux x86_64 cold/warm environment setup plus production proofs within 30 minutes, mandatory runner enforcement/CI aggregation, or the final license/distribution/three-OS gate. Those are explicitly assigned to Tasks 4 and 5 and are not claimed completed here. Stable tests, actual solver processes and sensitivity executions were not rerun by this reviewer; their preserved evidence and controller inspection are the basis for execution conclusions. No 32-bit target, complete runtime scheduling behavior or BudgetLedger state-machine proof is inferred.

## Task quality

Approved. The implementation, contracts, focused evidence and Task 4 handoff satisfy the Task 3 brief without a required fix.
