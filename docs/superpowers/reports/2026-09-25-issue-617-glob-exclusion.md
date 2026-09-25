# Issue 617 verification and reviews

Design and implementation plan were committed before production changes (`65b4b2a`), with three separate review passes in each document.

## Implementation self-review

1. Boundary responsibility: searched all `resolve_explicit` callers. The only production caller is `TargetHandler`, immediately after filesystem discovery. Removed only the raw-path exclude predicate; discovery still applies its existing glob implementation, include precedence, ignore rules and fixed exclusions. Kept core free of filesystem/glob dependencies.
2. API and errors: reviewed source, explicit file, line and symbol branches after the change. Normalization, root containment, Python-file checks and missing explicit-path errors remain in place. Public rustdoc and development documentation now require a prefiltered inventory; the direct literal-exclusion fixture supplies that inventory rather than silently retaining an obsolete contract.
3. Model correspondence: compared the four finite pattern interpretations against the real discovery adapter. The model proves idempotence of semantic filtering and exhibits raw-equality and escaped-pattern controls; it does not prove a general glob parser. The escaped-glob adapter is explicitly Unix-only, while the core regression is portable. All four generator registration sites are connected and ordered consistently.

## Test self-review

1. Regression sensitivity: the new core source/file/line test failed before the fix because the bracket-named file was removed. After the fix all 31 target-policy tests passed. Ordinary exclusions remain tested through the actual discovery pipeline; the finite corpus includes both kept and excluded explicit paths.
2. Observations: 20 corpus cases compare discovery paths, resolved paths/error class and public plan candidates, including operator text, byte span, line, diagnostics and truncation. A public run observes baseline success, one killed mutant and complete execution. Six rows differ under the deliberately broken raw-literal filter; escaped-pattern sensitivity is checked separately.
3. Portability and CI: selected a 1-byte fixture disk reserve, consistent with other tiny public-run fixtures, so Linux runner free space does not control a glob regression. Reviewed Unix escape scope and portable core coverage separately. Workflow contract tests passed 40/40; bounded Lean model/native builds, corpus freshness, sensitivity and stats passed. Resource evidence is committed next to this report.

## Independent review

The verify-lane reviewer independently inspected production callers, both discovery walks, unchanged override precedence, core API responsibility, finite model correspondence and CI registration. No correctness blocker was found. This review did not substitute for executed build/test evidence.

## Executed validation

Full workspace: 2290 passed, 0 failed, 22 ignored across 96 test groups. Exact CI all-target/all-feature workspace Clippy and locked parser Clippy passed, as did workspace/vendor formatting and diff checks. Lean peak was 1,285,616 KiB and the longest guarded command took 9,128 ms, below the unchanged 2,048-MiB / 20-second bounds.

Before publication, rebased onto main `efe3531`; resolved additive Lean registry conflicts by retaining both paging and glob-selection entries in matching order. Workflow contract tests passed 40/40 and the glob/paging/plan integration group passed 85 tests (1 ignored). Formatting and diff checks passed. No production conflict occurred.

## Published-branch integration with main `559219e`

Merged main into the published branch without rewriting history after issues 622 and 632 landed. The only conflicts were two CI model/entrypoint lists; retained both changed-context and glob-selection entries. Reviewed the automatically merged target code and all four registry sites: the glob-filtered discovery contract, changed-context selection and source-order compatibility additions are all preserved.

Fresh validation used the assigned cache after cleaning only core/CLI artifacts: workflow contract tests 40/40; `cargo test -p hoimin-cli --test lean_glob_selection_oracle --test plan -- --test-threads=1` passed with 1 ignored; exact CI workspace/all-target/all-feature clippy, locked parser clippy, workspace/vendor format checks and diff checks all passed. No Lean command or new semantic change was needed for this registry merge.
