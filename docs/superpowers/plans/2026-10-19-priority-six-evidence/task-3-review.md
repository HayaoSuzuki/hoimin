### Spec Compliance

- ✅ Spec compliant for Task 3 (#1004), reviewed at `6d78789eca05fad577b2a8dcce83077f9389ee06..8720b41a0d08f375d98bda7cfa6202be779f3226`. `RunConfig` has a private owner, checked construction and checked original-map deserialization; the shared view has no mutable counterpart (`crates/hoimin-core/src/config.rs:521`, `:599`, `:608`, `:616`). Arbitrary edits consume the owner and require acceptance again (`:627`).
- ✅ Raw normalization and mutable persisted-plan conversion enter the checked boundary; discovery remains validated before conversion (`crates/hoimin-core/src/config.rs:763`, `:1103`). Request-to-snapshot conversion retains exact-name matching and moves the already-checked common fields (`:683`). The consuming output and fingerprint-record operations affect fields outside the existing semantic validator, consistent with ruling R6 (`:582`, `:634`, `:642`).
- ✅ Public preparation receives checked requests before environment capture or filesystem fingerprint resolution (`crates/hoimin-cli/src/shell.rs:2351`). Strict review decodes checked configuration before canonicalizing the project root (`crates/hoimin-cli/src/artifacts/run_report.rs:61`, `:68`). Negative tests cover all six required malformed cases with filesystem failure controls (`crates/hoimin-cli/tests/checked_config.rs:16`, `:46`, `:64`; `crates/hoimin-cli/src/artifacts/run_report.rs:551`).
- ✅ Call-site migrations preserve ownership: plan conversion propagates errors before source resolution, worker-root rechecks reconstruct checked data before fingerprint resolution, and session preparation returns its resolved path rather than mutating accepted config (`crates/hoimin-cli/src/plan.rs:489`; `crates/hoimin-cli/src/fingerprint_inputs.rs:104`; `crates/hoimin-cli/src/shell.rs:987`). The reported all-target compilation/lint and affected tests support the migration beyond the visible call sites.
- ✅ No report/schema bump or unrelated feature is present. Guidance states the semantic guarantee and its filesystem limits (`docs/development.md:1854`; `docs/knowledge/design/selection-plan-verify.md:441`; `docs/knowledge/design/session-report.md:433`). Task 4's already-decided verbosity policy is outside this review.
- ⚠️ Cannot establish native Linux/Windows behavior, unchanged production Creusot proof validity, or final combined catalog/schema consistency from this task diff. These remain the controller's integration scope. Full progress decoder implementation and unchanged machine/fingerprint signatures are not shown in the diff; the passing progress suite and compile-fail execution/fingerprint guards provide task-local evidence, not a new audit of those implementations.

### Strengths

- The boundary is small and enforceable: public construction data is distinct from executable state, ordinary reads remain convenient, and no unchecked compatibility constructor, generic mutation closure, getter family, or test escape hatch was introduced (`crates/hoimin-core/src/config.rs:521`, `:544`, `:599`, `:624`).
- Original-map serde preserves duplicate known-key detection instead of first reducing input to a JSON value. The duplicate-jobs property fixture now makes both competing values individually valid, so its failure remains specific to duplicate keys (`crates/hoimin-core/src/config.rs:608`; `crates/hoimin-cli/src/artifacts/run_report.rs:718`).
- Required invalid cases are separately generated tests with Arrange/Act/Assert and literal or typed expectations. The absolute jobs case isolates the absolute ceiling by raising `max_processes`; the valid poison-input controls establish that I/O would fail if reached (`crates/hoimin-core/tests/checked_config.rs:21`, `:39`, `:58`; `crates/hoimin-cli/tests/checked_config.rs:46`, `:64`). The R6 construction-to-preparation composition does not manufacture an invalid executable type merely for testing.
- Mutation doctests cover scalar and nested-vector writes; additional compile-fail guards distinguish construction data from execution and fingerprint input (`crates/hoimin-core/src/config.rs:507`, `:513`, `:530`, `:536`). The construction/serde property compares generated values directly, and the metrics override expectation edits independent data rather than invoking the production override (`crates/hoimin-core/tests/checked_config.rs:105`; `crates/hoimin-cli/tests/plan.rs:1463`).
- Review command regressions check diagnostic content, exit 2, empty stdout, and absence of the review store, exercising the actual command boundary (`crates/hoimin-cli/src/review/tests.rs:635`).

### Issues

#### Critical (Must Fix)

- None found in this task.

#### Important (Should Fix)

- None found in this task.

#### Minor (Nice to Have)

- None found in this task.

### Checks and Evidence Limits

- Read the review package once in four sequential chunks. Reopened only the raw-conversion tail at `crates/hoimin-core/src/config.rs:1100` because the final hunk ended before the expression/function closed; confirmed it returns checked `Self::try_from` directly.
- Named risk: an overlooked constructor, mutable escape, or fallible plan caller could undermine acceptance. A focused symbol search over core configuration/machine and CLI source found the intended configuration implementations and migrated plan consumers. One guessed standalone fingerprint source path did not exist; no further filesystem crawl or test rerun was used to compensate. Compile-fail guards and the existing all-target lint log supply the remaining boundary evidence (`crates/hoimin-core/src/config.rs:530`, `:536`; `/tmp/hoimin-priority-six-task3-clippy.log:3`).
- Named risk: generic shared access could expose mutable environment metadata. A focused declaration/API search confirmed private request storage, private captured fields, validated serde construction, and borrowed-only names/digest access (`crates/hoimin-core/src/fingerprint_env.rs:36`, `:54`, `:97`, `:137`, `:143`).
- Named risk: the shared report reader could perform root I/O before new semantic acceptance. A focused reader-signature/context search confirmed typed decoding precedes canonicalization (`crates/hoimin-cli/src/artifacts/run_report.rs:61`, `:68`). Progress's decoder was not changed by this diff; its 128-test result is visible at `/tmp/hoimin-priority-six-task3-green-cli-integration.log:447`.
- Inspected existing RED evidence: six preparation cases reached missing-fingerprint I/O, six strict-review cases reached missing-root I/O, and both mutation snippets unexpectedly compiled before the fix (`/tmp/hoimin-priority-six-task3-red-preparation.log:20`, `:58`; `/tmp/hoimin-priority-six-task3-red-review.log:22`, `:60`; `/tmp/hoimin-priority-six-task3-red-doc.log:13`, `:21`).
- Inspected existing GREEN summaries: core 121, preparation 7, CLI library 757, CLI integration 666, machine/core 167, doctests 9, metrics recheck 1, and command recheck 6 passed. Counts match the report; final logs contain no warning/error lines. Existing ignored counts remain disclosed. All four mandatory commit hooks passed (`/tmp/hoimin-priority-six-task3-green-core.log:164`; `/tmp/hoimin-priority-six-task3-green-preparation.log:16`; `/tmp/hoimin-priority-six-task3-green-cli-lib.log:801`; `/tmp/hoimin-priority-six-task3-green-cli-integration.log:729`; `/tmp/hoimin-priority-six-task3-green-machine.log:200`; `/tmp/hoimin-priority-six-task3-green-doc.log:24`; `/tmp/hoimin-priority-six-task3-green-metrics.log:8`; `/tmp/hoimin-priority-six-task3-green-command-recheck.log:13`; `/tmp/hoimin-priority-six-task3-commit.log:1`). No tests, git commands, or product/index mutations were performed during review.

### Assessment

**Task quality:** Approved

**Reasoning:** The checked owner closes direct construction, serde acceptance, and post-check mutation while retaining the existing environment and preparation responsibilities. The migrated call sites and independent negative/property/compile-fail tests provide appropriate evidence for this task; no blocking correctness or maintainability defect was found.
