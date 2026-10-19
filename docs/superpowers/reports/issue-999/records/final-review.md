# Issue #999 — single final whole-branch review

Reviewed range: `f4460fc00317ca6468577d4f075c9d8848264af0..fb89c55c1ab6b851302f36d0fef7fd9f7697655a`.
Worktree: `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-999-creusot-budget-proofs`.

## Verdict

**Ready to merge: Yes, from this whole-branch implementation review.** No Critical, Important or Minor defect requiring a branch change was identified. The approved production arithmetic, three-target proof gate, tool/resource boundaries and ordinary distribution integration are implemented and supported by the retained acceptance evidence. PR publication and its ordinary fresh CI remain controller-owned steps; this verdict does not claim they have already happened.

The remaining intermittent Windows fixture-readiness cause is unproven. The final native run establishes the requested acceptance at its recorded revision; it does not establish a repair of that intermittent cause. That distinction is accurate in the final documentation and is not a reason to reject the arithmetic adoption.

## Strengths

- The production crate is the actual arithmetic owner. `crates/hoimin-budget-math/src/duration.rs:113` and `src/projection.rs:60` have common runtime bodies for stable and proof builds. Core's real execution and projection paths call that owner (`crates/hoimin-core/src/machine.rs:980,1474`); no second arithmetic implementation or compatibility forwarding layer remains.
- Contracts cover the complete public typed domain. The mathematical ceiling does not introduce machine-integer addition overflow; automatic timeout specifies both saturation steps and the five-second floor; multiplication handles both u128 overflow and Duration overflow; private reconstruction has a proved range precondition. The public projection has no provenance invariant. `is_shortfall` is strict field comparison even for externally assembled structs (`src/projection.rs:43`). Positive duration construction/access, serialized forms and named configuration errors remain coherent.
- The proof boundary is substantive: the manifest binds eight clauses, exact attached contracts, mathematical definitions, structural models, named generated files and goals. Fresh generation follows an owned-package clean and deletion of old generated output. Every generated file must appear in the final successful goal report. Each target is generated separately and replay uses `--no-cache`, so committed strategy files cannot supply old successful answers (`tools/verify_creusot.py:501`).
- Environment checking verifies executable selection, actual version output and exit status, built-file hashes, selected environment and the actual workspace's project configuration. The installation/project distinction closes the earlier effective-config gap (`tools/creusot_environment.py:123,156`). Direct artifacts and source revisions are pinned; receipts are correctly documented as drift evidence rather than a signature establishing trust in arbitrary images.
- Resource enforcement is explicit: effective prover commands apply 30-second GNU timeout and 2-GiB RLIMIT_AS, explicit `-j 2` limits solver scheduling, targets run sequentially and the CI job includes setup in its 30-minute limit. The runner cleans owned sessions on interrupted commands without blocking SIGTERM in launched children (`tools/verify_creusot.py:366,383`). Aggregate CI rejects selected proof failure, cancellation and unexpected skip.
- Extraction preserves normal builds and distribution. Creusot is selected by target cfg, not ordinary all-features activation. Root/fuzz locks, narrow license exceptions, release version rewriting, source archive coverage, SBOM required-component validation and sidecar cleanup include the new crate. The preserved license and documentation do not mistake dependency-audit success for a general legal conclusion.
- The evidence retains failed attempts and revision boundaries, rather than substituting later success for the failure history. Native cold/warm measurements, local ARM development results, normal builds and platform process tests are distinguished.

## Findings

### Critical

None.

### Important

None.

### Minor

None.

## Review method and inspected evidence

I performed five passes without subagents: (1) approved requirements, plan and ledger; (2) production arithmetic, contracts and generated correspondence; (3) environment, runner, resource cleanup and CI failure semantics; (4) APIs, platform changes, dependency/distribution integration; (5) evidence, documentation, prior findings and final readiness. This is the independent whole-branch review, separate from the implementers' recorded self-reviews.

I read the supplied final-review brief and rubric, approved specification/plan/global constraints, progress ledger and all seven Rulings, canonical adoption report and durable archive index. I used the supplied full-branch package's commit/file inventory and inspected the substantive current code and contextual diffs across the branch. Generated/repeated content was inspected structurally and by focused source/obligation comparisons, rather than treating every repeated generated line as an independent implementation.

Specifically inspected:

- All three math source modules and their manifest; arithmetic/serialization tests; extraction/caller changes; exact obligation manifest; representative generated constructor, reconstruction, product, projection and shortfall bodies, including input/type invariants and postconditions. The generated public-struct invariant constrains constituent Durations, not constructor provenance. All 75 strategy JSON files were parsed and contain pinned `z3@5.1.0` prover entries.
- Complete runner and environment validator; installer/configuration scripts, Dockerfile, primary tool manifest, smoke procedure and proof guide; runner/environment tests; workflow/selection/aggregate changes. The existing tests combine real recorded proof output with negative source/configuration cases and separate real process-lifetime tests. The orchestration fixture is not presented as execution of a proof engine.
- Full changed Windows diagnostics and suspended-thread code, lifecycle adapter changes, non-Linux workflow scheduling, package/release/SBOM changes and associated tests; root/fuzz lock diffs and licensing documentation. Capturing the ResumeThread error precedes test logging, preserving last-error behavior.
- Updated architecture/CI/dependency/release/knowledge documentation and the historical review dispositions. The source-provenance and inventory-count corrections are present.

Read-only evidence checks performed during this review:

1. Independently checked every entry in durable `SHA256SUMS`; all matched. This establishes archive consistency, not external authentication of historical execution.
2. Read archived native `5033e1bb` warm/cold summaries, setup reports and run metadata. Each target reports 25 fresh files, 59 proved named goals and 25 strategies; all generation argv contain the actual target and all proof argv contain `--no-cache --replay -j 2`. Warm setup-inclusive time is 247 seconds; cold is 693 seconds. The raw cold log contains the explicit image-cache miss and measured build RSS; all three cold proof logs end in `Proved (25 files)` with 59 named goal lines.
3. Confirmed by Git diff that math sources, root manifest/lock, proof runner/environment, target/strategy/obligation inputs and tool recipe are unchanged from `5033e1bb` through reviewed HEAD; the only change under the inspected proof-input paths is the guide. Confirmed `312be2c4..fb89c55c` is documentation/evidence only.
4. Read native `312be2c4` metadata: all nine non-Linux jobs succeeded. The raw final Windows Rust log contains both previously failing bounded Job Object tests, all three new paused-time boundary cases and actual lifecycle correspondence passing. Independently summed its 173 suite summaries to 2,886 passed, zero failed, 31 ignored. The job lasted 20m18s, within its 30-minute cap.
5. Read the five compiling sensitivity result records (generation exit 0, proof exit 1 and the corresponding named goal), package provenance and license-inspection records, and both final root/fuzz cargo-deny logs (`licenses ok, sources ok`). The package report records actual sdist extraction, locked wheel build, smoke and generated shipping SBOM; those artifacts were not rebuilt by this reviewer.

No test suite, solver run or heavy build was rerun. No new focused executable test was necessary: code inspection and retained evidence resolved the review's concrete questions. The checkout/index/HEAD were not changed; final status was clean at the supplied HEAD. This report is the only file written, outside the checkout.

## Eight-clause assessment

| Clause | Assessment |
| --- | --- |
| 1. Full-domain ceiling and safety | Exact integer ceiling contract; quotient/remainder implementation avoids overflowing `selected + jobs - 1`; no configured jobs/selection cap assumed. |
| 2. Automatic timeout | Exact clamped integer expression; shared function used by projection, configuration checks and actual mutant execution. |
| 3. Fixed/Auto | Both actual enum branches contracted; Fixed retains the positive duration exactly. |
| 4. Saturated capacity | Integer min contract; both intermediate-u128 overflow and representable product above Dmax return Duration::MAX. |
| 5. Reconstruction | Only private helper is range-restricted; division/remainder bounds justify exact casts and Duration construction. |
| 6. Shortfall | Strict greater-than; equality is false, independently of other public fields. |
| 7. Saved fields | Five separate preservation contracts agree with the body. |
| 8. Value invariants | Constructor and accessor establish positivity/exact value; deserialization uses the constructor; projection has no invented provenance restriction. |

The common explicit equality implementations preserve every existing field/variant. Structural DeepModel specifications and the omitted-field sensitivity case address their additional maintenance obligation. No hoimin-owned trusted specification, assumption or alternative proof runtime body was found.

## Rulings and deferred observations

All seven Rulings were considered:

1. **Independent test oracle:** justified. Minimal covering-wave properties and widened additive ceiling avoid mirroring the production calculation.
2. **ARM64 development environment:** justified and properly bounded. It did not replace native x86_64 acceptance, which now exists. A second host-artifact manifest is a real documented maintenance cost.
3. **timeout plus RLIMIT_AS:** justified. This makes the agreed wall/address-space bounds operational instead of relying on calibrated solver time. Documentation does not misstate it as aggregate RSS enforcement.
4. **Installation vs effective-project preflight:** justified. Actual runner invocations check the selected workspace configuration before generation and proof.
5. **Common explicit PartialEq:** justified. It avoids two conditional runtime derive paths; the added explicit comparisons are covered by structural specifications and regression evidence.
6. **Merge main before final integration:** justified. The reviewed base is the merged main revision; current-lock proof and integration acceptance followed it. Earlier proof-source revisions remain traceable.
7. **Two Windows harness threads:** acceptable as bounded test scheduling, with a meaningful cost and no demonstrated root-cause repair. The failure with two threads disproves sufficiency of that mitigation; the report explicitly retains this contrary evidence. Every test and six-second semantic assertion remains. The final 20m18s Windows job shows the setup remains viable within its existing cap.

Disposition of prior/deferred observations:

- **Effective project config, extra contract attributes, parent SIGTERM cleanup, inherited SIGTERM mask:** addressed in the current implementations and covered by retained scoped reviews/negative tests. No repeat of those identified defects was found.
- **Relative Coma source warnings:** addressed by absolute span generation; current native evidence has no source-span warnings. Historical fixture warnings are correctly preserved as historical data.
- **OKF adoption-record provenance:** corrected to consistent recorded revision/state in the new proof contract and audit catalog.
- **OKF dependency inventory mismatch:** corrected to the October 19 213/111 inventory; the linked source matches that description.
- **Windows lifecycle oracle premise:** repaired by making durable acceptance the prerequisite on every OS. Failure to establish it remains an infrastructure failure rather than a false semantic verdict.
- **Lifecycle exit guard:** the 14.25-second wait includes the unchanged run limit, shutdown grace and telemetry allowance. It starts after observing the premise and is not advertised as a whole-process cap. Failure still triggers bounded cleanup. Latest native evidence closes the scoped review's previously pending platform acceptance.
- **Intermittent Job Object readiness:** unresolved mechanism, accepted as an explicit nonblocking maintenance risk. The branch does not alter product limits or weaken its six-second tests. Bounded test-only phase/file/environment/native-job diagnostics are proportionate to preserving future failure context, though their added I/O may perturb timing. Do not describe the scheduling or diagnostics as a proven fix.
- **Missing raw RED terminal transcript for the exit-guard tests:** evidence limitation, not an unreported passing test. The exact command/result summary survives; raw GREEN/native logs and inspectable old/new bounds support acceptance. No independent replay of the historical RED is claimed.
- **Existing unsupported Python-shell warnings in release.yml:** unchanged by this branch and not evidence of a new proof/packaging failure. See declined items below.

## Acceptance evidence limits

No missing execution evidence blocks this branch review. The latest native proof and Linux full-CI evidence is at `5033e1bb`, and latest native macOS/Windows evidence is at `312be2c4`; it is not a fresh run at `fb89c55c`. The unchanged proof inputs and docs/evidence-only final commit justify using those results for this review. A normal PR proof run remains required by the implemented gate.

Historical suites and package builds were evaluated from preserved reports/logs, not reproduced here. I did not inspect every line of every historical raw log, independently derive compiler/solver correctness, or reauthenticate archived GitHub metadata against the live service. The optional Linux cgroup-v2 job was skipped and is not counted as exercised. None of these is represented as additional successful execution.

## Declined to judge

- **BudgetLedger transitions, reservation identity/ownership, persistence and full runtime state-machine correctness:** explicitly outside the approved arithmetic proof. Existing Lean/runtime regressions remain separate evidence; this branch does not prove those systems.
- **Arbitrary OS scheduling, strict aggregate whole-environment RSS, and the root cause or permanent elimination of intermittent Windows readiness failures:** arithmetic proofs and finite native runs cannot establish these claims. I assessed the changed diagnostics, unchanged semantic bounds and actual acceptance, but decline a universal timing or root-cause conclusion.
- **32-bit or other unsupported target proofs:** only the three approved target triples have generated/proved acceptance evidence; no broader proof is inferred.
- **General legal compatibility or redistribution compliance of custom Creusot builds, proof images or generated artifacts:** this review assesses the technical dependency boundary, pinned notices and bounded license documentation, not a legal determination. No new release or proof-image publication is in scope.
- **Repair of pre-existing release.yml unsupported Python-shell lint warnings:** the release workflow is unchanged and the warnings are independently documented; repairing unrelated workflow-linter coverage is outside this adoption change.
- **Live branch-protection settings, completed PR publication, merge or release:** the review is read-only and these external actions are not established by the branch diff. Mandatory aggregate behavior was inspected; publication is a subsequent controller task, and merge/release were not authorized within this implementation.

## Recommendations

Proceed to the controller's final publication/PR steps and ordinary fresh CI. Preserve the exact source revisions and the unresolved Windows-readiness qualification in the PR description. Future proof-tool or contract changes should retain the same full-domain review, fresh no-cache proofs and normal-build/distribution checks; the exact source inventory and 75 strategy files are deliberate maintenance costs, not evidence that future changes are automatically safe.
