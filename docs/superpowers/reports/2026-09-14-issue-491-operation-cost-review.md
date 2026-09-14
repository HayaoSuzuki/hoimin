# Issue #491: Remaining acceptance review and evidence

Base: 8b33167a049e3cae0fc05e96ccf2253c660b7023. Historical 21-gate evidence is not a new run. New executions, resource bounds, case modes and limitations will be recorded here.

## OKF: three reviews

1. Read the existing performance-shapes concept and its original design/review. It explicitly leaves extra Lean cost models incomplete; this is an extension of the same contract, not a second performance infrastructure.
2. Compared current source counters to previous claims. build_updates excludes sort/allocation, query_comparisons excludes model semantics, and retained heap is not peak heap. Keep these observation boundaries in the new worksheet.
3. Checked source/index rules and concurrent ownership. Add separate spec/report sources, retain historical revisions and hashes, and update current registry/guide hashes only after their owner's final edits.

## Design and plan: three reviews each

Design passes are recorded in the operation-cost design: existing-gap reconciliation, actual-step instrumentation and numeric/semantic bounds. Plan passes are recorded in the operation-cost plan: explicit ownership, boundary/semantic controls and actual broken operations with fresh release inputs. Each pass changed the selected work or strengthened its evidence requirements before implementation.

## Model and implementation: three reviews

1. Distinguished an abstract cost identity from the search algorithm. Added an arbitrary-branch binary-search recurrence, proved its cost at most the half-length recurrence, and proved that recurrence equals floor(log2 N)+1 for positive N. BuildTrace proves the per-leaf/ancestor count separately, and bounded retention reuses the existing theorem.
2. Reviewed semantic order and numerical preconditions. Generated ordered, duplicate and nonmonotone offset histories and independent eligible-event folds. Rust checks native conversion and intermediate allocation/product representability; it reads generated bounds instead of repeating their formulas.
3. Compared inventory assertions with required boundaries. A nonempty family check could lose 0/1, exact power-of-two or N/2N/4N rows silently; required all 56 cases and complete per-family sizes. Added independent annotation and flat-literal replacement checks alongside unchanged full candidate semantics in real broken paths.

## Proof development and sensitivity

Initial proof authoring found a changed induction name and recursive equation rewriting issues. These were corrected without increasing recursion depth or heartbeat limits. A subsequent isolated proof run failed only the deliberately false `buildUpdates 8 = 8` proposition, demonstrating detection of omitted ancestor updates. Restoring the true value 32 passed the general and boundary proofs. Finite cases search 0..32 and prove minimal witnesses: scan at3, full clone at1, eager byte build at1. This is a bounded witness search, not an unbounded asymptotic proof of the entire CLI.

Dependency compilation initially peaked 1,314,704 KiB under20s/2GiB. Isolated proofs passed under20s/768MiB (peak697232KiB). Generator attempts under768MiB ended with rss_limit/exit125 at818496,788992 and825952KiB, including isolated-module attempts; they do not count as semantic failures or passing evidence. The generator therefore uses the established2GiB ceiling with the unchanged20s deadline. No unbounded tactics or enlarged search domains were used.


## Tests: three reviews

1. The clone boundary test failed before instrumentation; the omitted-ancestor Lean proof failed before correction. Actual slow scan/clone/build paths then preserved semantic outputs but failed their bound predicates, instead of merely comparing fabricated bad counters. Both default and contracts builds passed all four cost adapter tests and the independent preflight test.
2. Reviewed the measured preflight failure and permitted-memory premise. Existing hashing uses fs::read for one whole file; revised peak allowance includes the largest-file buffer plus entries, then checked that real eager all-files×workers buffers still violate it. Reviewed clone width: the source adds N aliases plus required Sequence; the abstract N×N bad-copy witness is a lower bound, while the actual zero-callback-copy predicate counts every imported entry.
3. Reconciled final commands and environment. System Python3.12 lacked packaging and could not import the wheel tests (55 tests, one import error); the repository Python3.14.7 environment with process-monitor permissions passed all82 tests. Workspace/all-target/all-feature Clippy, format and diff checks passed. New code/CI was finalized before the successful suite; in-flight earlier failures remain recorded separately.

## Final execution evidence

- Lean proofs/cases passed; final generator output took9.383s, peak1,271,728KiB under20s/2GiB. Freshness and sensitivity passed under20s/768MiB; sensitivity reports56cases,0/1/7/8/9/16/32,three effects,max32events/max34queries,search0..32 and minimal scan/clone/eager witnesses3/1/1. This is the new modules and import dependencies, not a local aggregate build of all128 repository package modules.
- Rust: four cost tests pass under both default and contracts, covering56rows and three real broken families; preflight allocator test passes under both modes. Existing annotation snapshot1 and collection literal3 tests pass. Detailed peak values and commands are in the Rust cost review.
- All26 registered active gates executed and passed at registry SHA256 `36f30fa8aa02c0270e7822263229cb7650a460c79f79347f460ff6785f92752b`; original21 remained intact. Raw gate results are `/private/tmp/issue-491-final-gates/result.json`. Subsequent registry edits added only Lean cost reference metadata; gates and29 measurement shapes did not change. Registry validation passed again.
- Full Python suite82pass; Clippy `--workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check` and `git diff --check` passed. Module/generator inventory is128/31/28 sensitivity generators, confirmed from the passing workflow contract helper rather than counting unrelated root scratch modules.
- Fresh release lane:522 executions (29shapes×3sizes×3repeats×2binaries),174medians,87baseline/candidate and116within-binary growth comparisons. Committed expanded-measurement JSON contains actual environment/digests and raw-result hash/path. A selected production completeness mutation was killed with clean execution cleanup. See the input-axis report for exact selection and limits.

## PR: three reviews

1. Inspected complete Rust diff: instrumentation is test-only, the production replacement observer is an inline identity, and default KnownImports Clone remains derived. Parent independently reviewed the actual clone/builder paths, semantic comparisons, native bounds and allocator controls without further blocking findings.
2. Matched each acceptance item with a model, actual counter gate or bounded workload. Kept named operation vector, internal-fixture counter modes, public release observations, largest-file peak premise and unsupported native/OS proofs separate. No elapsed-time speedup or whole-CLI constant-memory claim is made.
3. Re-read final design/worksheet/report against generated rows,26 executed gates,522 fresh measurements and source inventories. Corrected stale package count using the workflow's actual roots, documented the metadata-only registry digest difference, and preserved historical21-gate evidence. Final source hashes and indexes are validated after contributors finish; publication uses the repository PR template.

## Remaining limits

This closes the missing validation infrastructure, not every possible asymptotic regression. Measurements are bounded macOS arm64 observations with concurrent-build noise; Linux/Windows performance and stack capacity are not established locally. CI covers the aggregate Lean build. The cost vector excludes sorting/allocation details and does not prove compiled Rust from Lean. Sources/AST memory and one-file fingerprint buffer remain input-proportional. Nested size0/1 share a valid minimal fixture. All counter modes remain internal-fixture; release timing/RSS are separate public measurements without fixed speed thresholds.


## Publication

Published [PR #544](https://github.com/tokyogas-tech/hoimin/pull/544) from enhancement/issue-491, implementation commit7213011. Delivery checklist is complete. Final OKF checks passed19 reserved/YAML pages,416 source IDs/footnotes,768 local links, root reachability, complete design/report indexes and all eight newly read performance-concept hashes. All35 nonempty generated cost sources also compiled with repository CPython3.14. This publication-state edit changes documentation only; implementation validation above remains the final code evidence.

## Hosted CI follow-up: ordinary output fixture isolation

PR544's randomized Linux run failed an unchanged paused-output unit fixture before its writer was entered (`Ok(2)`); the same head's ordinary Linux run passed that test. PR537 had shown the same symptom. The original test discarded stderr and the owned wrapper maps an inner error to exit2, so the precise original cause cannot be established from that log.

Review1 found a concrete isolation gap: output_failure_test_config used the CLI's default10GiB free-space reserve despite an existing host-independent ordinary-test helper. Adding that helper's reserve assertion to the paused test produced deterministic RED (10737418240 versus1). The shared output-failure config now uses the existing1B helper. Production limits, mutation-plan reserve and explicit disk-threshold tests are unchanged.

Review2 traced owned-writer lifetime and both tokio::select failure branches. A short-lived mutex-backed stderr capture now includes diagnostics for premature run completion or a dropped pause signal. This changes test failure visibility; stdout pause/release,50ms shutdown grace,1s post-cancel deadline, spool retention, execution cleanup and no-ack assertions remain intact.

Review3 checked all helper consumers and validation scope. All three affected ordinary output tests passed under both default and contracts builds, and all-feature lib/tests Clippy passed. Final formatting/whitespace checks and updated source hashes are checked before push. The parent did not claim the historical CI error was proven to be low disk or remove any output-lifecycle assertion.

Independent agent review found no blocker: capture lock scope and ownership are bounded, both early failure paths preserve diagnostics, ordinary output tests are isolated from host reserve, and explicit disk-stop tests retain their own settings. Hosted CI for this follow-up is tracked separately from these local results.
