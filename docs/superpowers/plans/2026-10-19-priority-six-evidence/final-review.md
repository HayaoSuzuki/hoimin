# Final whole-branch review

Reviewed 2026-10-19, base `1e6200d71237c6e445f337babf02f5f0354b610f` through head `4254936ca33240e2f69f6d612fbb408d92090c8a`, using the complete packaged `review-1e6200d7..4254936c.diff` and focused reads of the corresponding source. This is the single final branch review, following the requested `requesting-code-review/code-reviewer.md` template. No subagents, Cargo operations, product edits, index changes or HEAD changes were used. The controller authorized the two narrowly scoped existing-binary probes below in `/tmp/hoimin-priority-six-final-review`.

## Strengths

- **Safe acquisition has one implementation boundary.** `plan/source.rs:31` moves root acquisition and reads to blocking work, retains one capability reader per operation, and preserves lazy opening for candidate-ID error priority. Both plan consumers use it. `workspace/root.rs:294` reaches the existing no-follow/nonblocking open and same-descriptor regular-file check; hashing streams through `Hasher::update_reader`. The synchronized FIFO/symlink regressions exercise both consumers, and the public source-stage heap gate excludes later analyzer allocations.
- **Checked configuration retains a small API.** `hoimin-core/src/config.rs:599` and `:608` share semantic acceptance for construction and original-map deserialization. There is no `DerefMut` or mutable nested escape. Consuming output/fingerprint replacements do not alter the validated semantic fields, and request-to-snapshot environment typing survives. Session path resolution now returns prepared filesystem state separately instead of mutating accepted configuration. Strict review decodes checked configuration before canonicalization; progress retains its intentionally opaque configuration type.
- **Human presentation and verdict ownership remain separate from logical data.** `human_text.rs:20` owns the escaping policy, including the explicit baseline LF/TAB exception; preview, diagnostics, review, progress and complete mutant rows reuse it. Trusted `bin_name` closes the argv[0] provenance path. `hoimin-core/src/report.rs:509` computes wire status from outcome and `:541` checks/discards received status; summary/resume do not gain a second classifier. The tests cover direct/tagged serialization, duplicate and contradictory status, close-timeout precedence, public readers and real parallel completion rows.
- **Janitor accounting is substantially clearer.** `workspace/owned/janitor.rs:93` folds one final outcome per inspected root. Acknowledged deletion progress survives nested errors, ordinary details yield to failure details at capacity, and startup observations no longer contaminate execution cleanup evidence. Runtime verbosity projects the same summary before all three encodings. Root, entry, scan and detail counts have distinct documented meanings.
- **Graph work is shared and bounded.** The new private graph module uses path identity, iterative forward/reverse traversals, push-time marking and reverse component order for the approved H256 calculation. Existing dynamic flags are OR-preserved; lazy and missing dependencies remain cancellation checkpoints without becoming edges. Small-graph closure/path references, real source fixtures, depth 255/256, cycle-prefix boundaries, renaming, old-exclusion containment and operation counters exercise different failure modes.
- **The durable evidence avoids overstating results.** Release CLI time, instrumented requested heap and sampled RSS are distinguished. I independently recomputed all 16 time/heap medians from 96 rows, checked the eight fixture file/byte totals, identical candidate hashes, exact runner/probe/fixture hashes and return-to-input-baseline heap observations. All 63 archive entry hashes, 24 linked measurement hashes and 16 final check receipts/log hashes matched. Peak requested heap is correctly reported as unchanged.

## Issues

### Critical (Must Fix)

None found.

### Important (Should Fix)

1. **Schema-9 readers silently accept the retired disk field.**

   - **Location:** `crates/hoimin-core/src/report.rs:16` and `:29` (`DiskRunSummary`); published opposite contract at `docs/json-schema/run-event.schema.json:799` and `:814`.
   - **Problem:** `DiskRunSummary` derives `Deserialize` without `deny_unknown_fields`. Replacing `stale_roots_reclaimed` with required `janitor` therefore makes the retired field an ignored unknown field. A schema-9 report containing both `janitor: null` and `stale_roots_reclaimed: 999` is accepted by the actual JSON and JSONL progress readers with exit 0. The published schema rejects that shape. The required-new-field test covers omission, but does not cover a partially migrated producer retaining the old field.
   - **Impact:** This silently accepts a mixed old/new report shape despite the branch's explicit no-silent-schema-mixture constraint and removal notice. A custom producer can continue presenting a contradictory legacy count while the strict reader ignores it. This does not change mutation classification, but it breaks the promised wire acceptance contract and masks an incomplete migration.
   - **Fix:** Reject unknown fields on this changed wire object (or explicitly reject the retired member if there is a separately justified extensibility policy). Add independent JSON and JSONL reader rejection cases with positive current-schema controls; keep explicit-null acceptance and missing-janitor rejection.
   - **Reproduction:** Starting from the schema-v9 current report/event goldens, add only `disk.stale_roots_reclaimed = 999`, then run `hoimin progress VALID MODIFIED --format json`. Both modified formats returned exit 0; both valid controls returned exit 0. Exact argv, stdout/stderr, source head, binary version and hash are in `/tmp/hoimin-priority-six-final-review/retired-field-receipt.json`. Initial argument/JSONL fixture setup failures are retained in separate receipt fields, not counted as evidence; the final JSONL probe uses the existing event golden including `mutant_started`.

2. **A valid young pre-lease staging root is classified as an invalid lease.**

   - **Location:** `crates/hoimin-cli/src/workspace/owned.rs:5708`, `:5789` and `:6788`.
   - **Problem:** For an empty canonical staging directory left by a crash before lease creation, the successful age check returns `Young`. `directory_timestamp_is_stale` discards that reason and returns false. The caller then falls through to the missing lease branch and finalizes the root as `unresolved / lease_invalid`. This is a recognized lifecycle state: the existing `empty_staging_root_from_a_pre_lease_crash_is_reclaimed_after_a_day` test at `owned.rs:2655` already accepts the same unmarked directory once its age threshold is reached.
   - **Impact:** During the normal waiting period, default output reports a failure-like root/path and the structured preserved/young count is wrong. Repeated pre-lease crash remnants can again fill default output with roots whose applicable hold is their age. Deletion eligibility and run exit policy are unchanged; the defect is the new reason/outcome projection. Task 4's blanket statement that a missing lease cannot be an ordinary hold overlooks this special empty-staging state; this is a design gap carried into the implementation, not grounds to relax the general missing-lease rule.
   - **Fix:** Retain the typed age result for the existing empty unmarked staging special case. After successful empty-directory/age inspection and an absent lease, finalize a young directory as a `Young` hold while preserving exactly the existing refusal to delete it. Keep nonempty staging, unexpected missing leases and failed inspection/age evidence unresolved. Cover young default and verbose projections independently and retain the stale-empty reclamation control.
   - **Reproduction:** With an isolated task-owned `TMPDIR`, a control run had zero inspected roots. I created an empty mode-0700 `.staging-UUID` directory under that run's managed parent. The next run reported `inspected_roots=1`, `preserved_roots=0`, `unresolved_roots=1`, `unresolved_by_reason={"lease_invalid":1}`, and one default-visible missing-lease diagnostic. Its directory remained unchanged. Setting only its mtime to 25 hours earlier made a following run reclaim it (`reclaimed_roots=1`, `unresolved_roots=0`). All three runs exited 1 because `/usr/bin/true` survived the mutation, not because of cleanup. Exact argv, before/after observations and reports are retained in `/tmp/hoimin-priority-six-final-review/staging/receipt.json` and adjacent stdout files.

### Minor (Nice to Have)

None found.

## Plan alignment and cross-task assessment

The six issues are implemented within their intended architectural boundaries, subject to the two reporting defects above. No unrelated command, text alias, exit policy, line-selection or visibility-index feature was introduced. Source and graph changes do not add identity/fingerprint adapters. Report/event schema 9 and shipping version 0.14.0 are coordinated across manifests, schemas, current fixtures, user guidance and repository skills; saved plan/session/fingerprint/candidate formats retain their documented versions.

The accepted R4 depth rule is faithfully implemented: an acyclic leaf has height zero, cyclic terminal weight is component size, all outgoing component edges contribute, and values saturate at 256. This preserves actual cycle membership and avoids blanket rejection of short prefixes. Additional conservative exclusions are a documented, user-approved semantic change, not an accidental regression. I found no implementation defect in this rule or its downstream dynamic/visibility use.

The baseline multiline exception and trusted executable display name are explicit presentation tradeoffs. Generic JSON Schema validates status shape while typed Rust readers validate its meaning; duplicating the classifier in JSON Schema is not needed. Neither the baseline exception nor schema-only status limitation explains the unrelated retired-disk-field acceptance in issue 1.

R13's historical references remain real navigation/evidence limitations. I inspected the checker branches requiring unchanged base entries and missing-at-base paths, the explicit 35-reference/22-entry inventories, and the strict handling of newly changed metadata. Preserving old read-time hashes instead of claiming fresh proof or source verification is appropriate here. I found no new broken reference masked by this exception; inaccessible historical content remains unverified as listed below.

Protected #999 proof/tooling/workflows/math and separate fuzz target/harness source paths are unchanged across the full branch range. This supports the stated source-scope preservation; it is not a new proof execution or a proof of all edited Rust behavior.

## Test quality and validation evidence

The existing integration evidence is substantial and its logs/receipts match the archive: workspace **3,334 passed / 33 ignored**, core contracts **688 / 2**, CLI contracts **2,183 / 28**, Python **1,201 passed / 11 skipped**, and **40 performance gates**. Formatting, clippy, Ruff, ty and reference checks have successful retained receipts. Those are existing native macOS results at product head `3aaf82b0`, followed by documentation-only changes; I did not rerun or relabel them as fresh runs at the final review head.

Behavioral tests generally exercise real boundaries rather than just mirroring helpers: synchronized subprocess races, public inert preview with hostile filenames, poison-filesystem configuration controls, compile-fail mutation examples, original-map duplicate rejection, spool/parallel/reuse paths, and independent graph reference algorithms. The missing cases are concrete: old-and-new disk fields together, and the young form of the supported empty pre-lease staging state. The existing stale-staging test alone cannot establish the latter classification.

My focused probes used the existing `/tmp/hoimin-priority-six-target/debug/hoimin`, version `hoimin 0.14.0`, SHA-256 `18ff52a217a59d8838cf12c5267565afca3476ecb9e2f1516040c12fde37bbbc`. Source HEAD was `4254936ca33240e2f69f6d612fbb408d92090c8a` and the checkout was clean. The binary was not rebuilt for review; this identifies the tested artifact rather than claiming an independently reconstructed binary/source correspondence. The observed behaviors also follow directly from the reviewed source branches.

## Recommendations

- Address the two findings together in the allowed final fix wave. Keep the fixes at `DiskRunSummary` decoding and janitor evidence classification; neither requires changing deletion eligibility, execution outcome ownership or saved formats again.
- Use focused regression cases with valid controls and one behavioral case per test. For the staging fix, explicitly retain nonempty/malformed-root refusal and the existing stale-empty removal behavior.
- Archive this report, the focused probe receipts and any scoped fix/re-review evidence before publication. Complete automatic Linux and R12 native Windows/macOS CI on the final published head; do not present the local checks as those results.

## Declined to judge

- **Native Linux/Windows behavior and final hosted macOS CI:** only native macOS local evidence is available at this checkpoint; automatic PR CI and R12 dispatch are still pending. Windows delete-pending semantics were read and their limits considered, but native execution is not established here.
- **Fresh Creusot/Lean proof execution and whole-implementation formal correctness:** protected proof inputs are unchanged and existing corpus adapters have retained passing Rust results, but this task deliberately did not execute or extend proof tools. No new theorem-level assurance is claimed for the source reader, janitor or SCC implementation.
- **Contents of 11 unavailable historical commits and the old read-time snapshots behind 103 differing hashes:** local objects/snapshot evidence are insufficient. The 22 source entries and hash limitations are retained explicitly under R13; they are not treated as verified current sources.
- **External URL availability and OKF heading-fragment correctness:** the retained checker verifies local paths and scoped metadata, not these targets; no network or complete fragment audit was performed.
- **Universal performance/memory improvement, arbitrary filesystem hard deadlines, or Unicode visual-spoofing prevention:** the branch explicitly limits its claims to fixed-workload measurements, cooperative filesystem behavior and control/line-separator escaping. The stronger properties have neither an implementation claim nor evidence in this change.
- **Wheel-build/fuzz-campaign outcomes and actual PR/ref/CI publication state:** these actions/results have not occurred within this review scope. They are not inferred from workspace tests or archived plans.

## Assessment

**Ready to merge? With fixes.**

The main architecture, source safety, immutable configuration, outcome-owned status and shared graph analysis are sound on the reviewed evidence. Fix the schema-reader mismatch and young unmarked-staging classification, then complete the scoped re-review and pending publication CI; there is no merge or release authorization in this review.
