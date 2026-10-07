# Recent default promotion review

Base8437243. Design and plan each contain five reviews. The user's opt-out request
supersedes the two original reports' opt-in rollout decisions; their mutation and
formal-evidence boundaries remain unchanged.

## Implementation self-reviews

1. Exact policy delta: production adds only MethodCallRemove and FunctionBodyReturnConstant in Default. Current52, historical43, full71; no selector families, guards or ranking changes.
2. Normalization precedence: omitted raw selectors use Default; explicit selectors build their own set; exclusions remain last. Individual exclusions preserve other descriptors/ordered IDs.
3. Persistence: preparation clones the saved configuration. Public preparation now asserts the exact previous50 set, separately from successful saved-plan execution. Session fingerprints still distinguish different operator sets.
4. Resource behavior: adding candidates can consume existing limits earlier. Runtime/README fixture failures were traced to two additional valid return mutants; tests now expect allthree, and documentation smoke budget covers all32 retained fixture candidates. The metrics-write failure comparison pins its unrelated arithmetic fixture to binary_add_sub and asserts completion; with three defaults and a two-mutant budget, not_run event order had become nondeterministic. Product limits and incomplete-run handling remain unchanged.
5. Documentation/CI: README count, opt-out examples and knowledge catalog describe52 and preserve historical reports. CI explicitly checks the selection proof under its existing resource guard; Python contract verifies gate placement.

## Test self-reviews

1. RED/GREEN: unchanged core baseline16 passed; expected52 assertions failed against old50 (14 passed,2 failed). Adding the pair made all16 pass.
2. Membership independence: literal52 CLI inventory and core exact-nine-additions versus historical43 guard against accidental promotions; remaining opt-in families remain excluded.
3. Public observations: both new operators occur in the shared source. Nine individual exclusions retain every other candidate descriptor/order; explicit full selection equals defaults. Removing the recent pair equals the explicit previous50 candidate array and set.
4. Saved configuration: the independent reviewer noted that top1 success alone does not exclude extra rediscovery. Added an exact public prepare_verify configuration assertion; top1 execution now supports compatibility only. The first test draft used the core OutputFormat instead of CLI OutputFormat and was corrected before execution.
5. Regression breadth: the first full no-fail-fast run isolated four stale fixture assumptions (one documentation budget, three status arrays). Updating them preserves default execution and strict completed-result assertions. Focused reruns passed the default-selection/report cases and the three corrected session/runtime cases. They exposed the additional metrics fixture budget/order assumption; that fixture now keeps its explicit arithmetic scope. Final whole-workspace and quality results follow below.

## Independent review

No production correctness defects. One minor observation-strength issue was addressed
by observing the public prepared configuration's exact saved operators, rather than
claiming that successful execution alone proves the rediscovery set. No deferred
review findings. Static review did not claim test execution.

## Local source trials

[trial.py](trial.py) performs discovery only, copying installed modules to temporary
roots and comparing full candidate arrays. [observations.json](observations.json)
records versions/source hashes. packaging.version26.3:476 ->484; iniconfig2.3.0:34 ->38.
Both recent opt-outs recover the previous50 array exactly; neither run truncates at
10000 candidates. This is not a runtime benefit or broad performance measurement.

## Final verification (2026-10-07)

`cargo test --offline --workspace --no-fail-fast`: exit0, 2708 passed, 22
intentionally ignored. Final workspace fmt and CI all-targets/all-features Clippy
passed. Python CI contract tests:142 passed. Lean proof check: exit0 under20s/2GiB.
OKF:25 concept YAML headers, four reserved files, new source link/hash checked.
The initial failed runs and corrected test assumptions above remain part of the
evidence; their outcomes are not counted as success. Precommit repeats required
workspace/vendor quality checks. PR creation, stack linking and cargo clean follow.
