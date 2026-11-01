# Issue999 adoption design tree

Current repository main: b65f62834b35a9ff85795b041e9687b21e5c51ba (PR1000 merged). Existing worktrees and user untracked files preserved.
User request: apply Creusot to actual production code and continuing development workflow, beyond the original PoC-only issue; use grilling to reconsider specification.

## Established requirements
- Same real implementation used in production and proof. No proof-only duplicate or compatibility shim.
- Preserve current public typed input domain, including usize/Duration extremes, and runtime behavior.
- Existing Lean proofs/oracle checks, runtime contracts and tests remain complementary.
- Stable/MSRV1.99.0 stays unchanged unless separately justified; dated verificationnightly is separate.
- Existing user practice: minimum5selfreviews perstage; AAA,onecaseperfunction execution,parameterized/property tests; conserve memory/disk; issue-topic branch,no codexprefix; throughPR.

## Settled user decisions
Q1: Proof failure/incomplete/overall timeout fails PR CI. DedicatedLinux proof environment. Ordinary product builds/distribution do not require proof tools.
Q2: Mandatory scope is full budget projection plus auto timeout arithmetic; Ledger reservation/release state machine proof deferred to an independent extension.

## Pending frontier
Q3: Forbid hoimin-authored unproved trusted/external assumptions in targetlogic; official standardlibrary specs/compiler/solvers explicitly trusted. Prefer equivalent production implementation refactoring for unsupported APIs.
Q4: Prove all3shippingtargets Linuxx86_64/macOSarm64/Windowsx86_64, via proposedLinux-host cross-targetverification; source-supported but not yet executed.

## Subsequent branches to settle after current round
- CI fail-closed precise mechanics, sensitivity tests and proofartifact freshness; exact solver/version preflight necessary because missing solver can warn/continue.
- Resource limits, initial setup vs warmproof cost; actual costs not measured yet.
- Concrete architecture/integration choice after code/tool facts, maintained proof surface and acceptance measurements.
- Final shared-understanding confirmation before implementation, as explicitly required by user-invoked grilling.

## Facts
- Projection warns; it does not enforce time budgets. auto_timeout also governs actual RunMutant execution.
- Creusotv0.14.0 is current directlytaggedrelease; specifiednightly2026-08-03 isrustc1.99.0-nightly, not evidence of MSRVmismatch.
- Official specs include Duration nanosecondmodel/checked arithmetic and NonZero. Some convenience/conversion APIs require integrationwork; compilation/proofsuccess unverified.
- --target is forwarded throughcargo check/rustc; nativeofficialCI doesnotdemonstrate requested3cross-targetruns.

Q3..Q7 allapproved. Q5actualproductionpurecrate; Q6freshsourceallobligations+cache-free+fixedtools; Q7perprover30s/2GiB/max2parallel/job30minincludingenvironment. Alluserpolicyfrontierssettled; finalwritten-spec/shared-understandingconfirmationpending. Spec: /tmp/hoimin-issue999-design-mnk8miii/2026-10-19-issue-999-creusot-adoption-design.md

FinalQ8written-spec/shared-understanding confirmation requested. REQUIRED response pending; do not install/build/changeproductorissue until confirmed. Questionlink points to completewritten107linespec, userwillthenauthorize issueupdate/plan/application/tests/PR. Finalskillpauseexplanation given with exactgrillingSKILLlinkandquote. Originalissue archived issue-original.json. No productfilechanges.

## Final approval
User approved the complete specification and execution with 「はい、進めて」. Q8 is resolved; proceed through implementation and PR without another approval checkpoint.
