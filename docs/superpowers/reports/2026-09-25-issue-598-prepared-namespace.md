# Prepared namespace verification and review (#598)

Baseline: `4e3bc2a97cad2e4ce8ae2b11c970fc2f5dff9bb2`. Worktree: issue-598. Reviews below are self-reviews by the implementation agent, not independent approvals. Execution outcomes are recorded separately from review observations.

## Design reviews before implementation

1. **Semantic boundary review.** Compared the issue's 12-case runtime identity model with the proposed static class flag. Found that the empty custom-metaclass case cannot retain its historical candidate expectation: static proof has no execution result. Fixed the design to model runtime identity and conservative eligibility separately and preserve the runtime-positive/static-negative control.
2. **Lookup precedence review.** Read both resolver functions and class/comprehension scope construction; ran an isolated CPython 3.14 nonlocal fixture. Observed a prepared `any` overriding an explicit class `nonlocal any` via `LOAD_FROM_DICT_OR_DEREF`. Fixed the design to preserve global bypass but avoid asserting nonlocal bypass; methods and comprehension bodies skip the entire class before either directive handling or namespace guard.
3. **Coverage/precision review.** Traced generic class header traversal and declaration annotation lookup. Found the policy must cover class-visible annotation scopes and first comprehension iterables, but must not taint the class's own bases. Added explicit header/declaration boundaries and documented precision loss for `class C(object)` / `metaclass=type`, arbitrary mapping behavior, and unsupported dynamic builtins mutation.

## Plan reviews before implementation

1. **Spec-to-task review.** Mapped each design boundary to Task 1/2; found public `run` could complete with zero candidates without independently demonstrating baseline Python semantics. Added explicit CPython endpoint probes and all three injection patterns to public run coverage.
2. **Execution/API review.** Inspected existing analyzer helper signatures and CI Lean runner. Found the draft's generic test instruction omitted concrete test code and the resource-guard path; added the exact regression shape and project guard invocation. Tests must select a specific operator, not count unrelated mutation families.
3. **Evidence/independence review.** Checked planned corpus fields against the replay adapter and review obligations. Found an eligibility-only corpus could pass despite incorrect fixture assumptions. Added runtime identity observations generated from Lean, closed schema validation, exact positive candidate spans/pairs, and distinct implementation/test reviews. No Python helper edits are planned, so Python mutation workflow is not applicable.

## Execution

Pending implementation. Root reported baseline `cargo test --workspace`: 2265 passed, 22 ignored across 94 suites; this agent has not rerun that baseline.
