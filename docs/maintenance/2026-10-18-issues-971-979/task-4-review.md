### Spec Compliance

- ✅ Spec compliant. The production hunk removes only `RunState::with_fingerprint`; `RunState::new` remains, and the test helper now constructs it without a constructor fingerprint (`crates/hoimin-core/src/machine.rs:346`, `crates/hoimin-core/tests/machine.rs:5504`). The current constructor list is updated without modifying historical documents (`docs/development.md:205`). All three files named by the brief have corresponding hunks.
- ✅ The retained fingerprint field and `None` initializer remain (`crates/hoimin-core/src/machine.rs:260`, `:311`); preflight stores the evidence before selecting the session path (`:1491`). Fresh creation reads it with `MissingFingerprint` validation (`:764`), resume loading does likewise (`:1499`), and resume-to-fresh calls the same creation function (`:1479`). No compatibility adapter, forwarding API, schema conversion, or unrelated production behavior is introduced.
- ✅ Added tests use AAA with direct behavioral assertions: bounded 64-case arbitrary fingerprint properties, named finite resume-fallback/mode cases, explicit missing-evidence errors, disabled-session worker progress, and stopping before preflight (`crates/hoimin-core/tests/machine.rs:2485`, `:975`). The helpers arrange evidence without hiding scenario-selecting assertions (`:5504`). No removed-symbol test is added.
- ✅ Five distinct, substantive reviews are recorded for each required stage (`.superpowers/sdd/2026-10-18-issues-971-979/task-4-report.md:7`, `:25`, `:33`, `:64`). Passing characterization before deletion is documented separately from the resolved formatting/lint failures (`:19`, `:56`).
- ⚠️ Cross-task integrated preservation of native argv, candidate selection/order/IDs, and cancellation/resource ownership cannot be established from this task's narrow diff. This task changes none of their production paths; the controller's integrated verification remains the appropriate check (`.superpowers/sdd/2026-10-18-issues-971-979/task-4-report.md:70`).

### Strengths

- The new pending-preflight fixture exposes the real event payload, allowing tests to distinguish preflight evidence from old constructor initialization while preserving the existing completion-driving helper (`crates/hoimin-core/tests/machine.rs:5504`).
- The fingerprint properties compare actual `LoadSession` and `BeginSession` request payloads, including all six named fresh-fallback reasons; the disabled test asserts nonempty worker effects, avoiding vacuous success (`crates/hoimin-core/tests/machine.rs:2485`).
- The deadline test observes effects throughout reporting and cleanup across disabled/fresh/resume configurations, strengthening the existing ownership boundary without changing production stop behavior (`crates/hoimin-core/tests/machine.rs:975`).

### Issues

#### Critical (Must Fix)

- None.

#### Important (Should Fix)

- None.

#### Minor (Nice to Have)

- None.

### Checks and Review Boundaries

- Read the supplied diff once; no git commands, Cargo commands, test reruns, or subagents were used. No changed function was reread to reconstruct a truncated hunk.
- Named concrete risk requiring reads outside the diff: deleting a public initializer could remove the only source of fingerprint state or break its lifetime across resume-to-fresh. Focused checks inspected only the retained field/initializer and fingerprint consumers/assignment in `crates/hoimin-core/src/machine.rs:260`, `:311`, `:764`, `:1479`, `:1491`, and `:1499`; the evidence confirms preflight ownership and retained fallback state. No web/external documentation reads were necessary.
- Named concrete risk requiring a source search outside the diff: remaining callers or a current documentation reference to the removed API. An exact-word search in `crates` and `docs` found only the current task plan and two historical Issue 487 records (`docs/superpowers/plans/2026-10-18-issues-971-979.md:74`, `docs/superpowers/specs/2026-09-11-issue-487-resource-report-policy-design.md:39`, `docs/superpowers/plans/2026-09-11-issue-487-resource-report-policy.md:54`), consistent with preserving history.
- Read existing verification evidence: characterization machine 95 passed (`/tmp/hoimin-design-audit-5d3aee95/task-4-characterization-machine.log:115`); characterization preflight 5 passed (`task-4-characterization-preflight.log:12`); contracts-enabled suites 3 + 95 + 4 + 25 + 6 passed (`task-4-core-contracts.log:10`, `:111`, `:121`, `:152`, `:164`); final corrected preflight subset 5 passed (`task-4-final-preflight-after-lint.log:12`). The inspected successful test logs contain no warnings/errors.
- Read final hook evidence: workspace/vendor fmt and clippy all passed (`/tmp/hoimin-design-audit-5d3aee95/task-4-commit-hooks-success.log:1`). The broader behavioral run preceded the assertion-spelling-only correction; the affected final subset and all final hooks cover that correction (`.superpowers/sdd/2026-10-18-issues-971-979/task-4-report.md:68`).

### Assessment

**Task quality:** Approved.

**Reasoning:** The implementation deletes the redundant API with a minimal production change and preserves the state needed after resume-to-fresh. Behavioral characterization, focused post-change evidence, direct payload assertions, and updated current documentation satisfy Task 4 without an unresolved defect.
