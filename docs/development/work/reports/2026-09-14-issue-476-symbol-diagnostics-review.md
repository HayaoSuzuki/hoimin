# Issue #476: symbol diagnostics review

Base: 8b33167. Work performed on macOS in the issue-476 worktree. User authorized autonomous stages and PR; reviews below are self-reviews, not independent agent approval.

## OKF stage reviews

1. Read overview, selection contract, workflow and source indexes. Finding: candidate emptiness does not establish definition absence. Preserve this distinction in the existing selection concept.
2. Compared source contract to `TargetHandler::resolve` and plan create/verify. Finding: changed intersection removes explicit targets and verify narrows discovery. Add validation before intersection rather than claiming discovery alone covers all selectors.
3. Reviewed provenance requirements and original-source inventory. Finding: new spec and report both require indexed source metadata; plan is outside the source inventory. Schedule final hashes after source edits, retain historical source metadata.

## Design stage reviews

1. Acceptance coverage: function-only validation would miss class, nested and async definitions. The AST collector must handle both definition kinds and normal control-flow traversal.
2. Boundary review: candidate-limit and verify requested-file shortcuts can skip invalid selectors. Shared target resolution precedes both and is selected.
3. Resource/error review: recursive AST traversal requires the existing depth guard; malformed syntax cannot prove a missing definition. Added guard, disposal and explicit parse-error handling. Documented additional parse and timeout boundary rather than claiming a new global time bound.

## Plan stage reviews

1. Spec trace: initial test list needed clean Git, multiple selectors and edited verify manifest cases; all added to Task 1.
2. Interface review: helper returns a set of complete qualnames; target code compares exact strings and reports resolved file plus original matching selectors. Existing candidate descendant filtering remains unchanged.
3. Scope and verification review: no schema, Python runtime or unrelated core selector restructuring is needed. Plan includes format, focused regression, existing suites and final OKF hash/link checks.

## Implementation stage reviews

1. Read the diff against the design and public tests. The three missing-definition regressions failed before implementation: plan returned 0, clean-Git plan returned 0, and edited verify returned 1. After implementation all six initial cases passed. Existing-definition controls cover class, method, nested class/function, async function and package init. Found test fixture environment missing; linked the pre-existing controlled `.venv` locally (not part of the commit).
2. Re-read error and resource paths. Root-relative source reading matches analyzer containment policy, parser recovery trees are disposed iteratively, and the existing depth guard precedes recursive visitor and ordinary drop. Found missing public assertions for syntax uncertainty and exact-name/assignment boundaries; added dedicated regressions. No production correction was needed.
3. Re-read cross-command call sites and selection invariants. Shared validation precedes Git intersection and verify's requested discovery subset; the verify regression changes only a selector in an otherwise generated manifest and checks that no baseline marker appears. Original selectors are indexed by qualname, so same-named selectors in different modules may appear together in a diagnostic; resolved path disambiguates the failure. Kept this documented behavior instead of introducing repeated selector resolution.

## Validation observations

The first test invocation failed only because the worktree lacked its controlled Python path and is not red-phase evidence. A subsequent shared-cache invocation executed zero tests after another worktree rebuilt the same package; it is explicitly not verification evidence. The next compile executed six tests and produced the three intended failures above. A fresh compile after implementation ran all six successfully. Final checks use a dedicated target directory with debug info and incremental compilation disabled to prevent cross-worktree cache contamination and bound disk usage.

## Verification stage reviews

1. Checked actual test counts and failure causes against the logs. Dedicated-target focused suites passed: analyzer handler 40, target handler 44, plan 63 with one existing ignored test. `cargo test --workspace` exited 0 (75 harness summaries, 1,836 passes including child harnesses, 19 ignored); this total is not a count of unique test functions. No skipped native test is claimed as native execution evidence.
2. Ran workspace/all-target/all-feature Clippy. It rejected a local type declaration after statements; moved the declaration ahead of statements, without changing control flow. The rerun passed, as did locked no-deps parser Clippy. Workspace and vendored-parser formatting passed. Full core contracts passed. Initial full CLI contracts had one existing disk-monitor timeout; that fixture uses `--file` and no symbols, so the new validation returns immediately. The exact isolated retry passed in 0.28 seconds. Concurrent host load is a possible cause, not an established root cause; the original failure remains recorded.
3. Rechecked the README and OKF wording against the current call graph, including early verify manifest/selection validation. The guarantee is rejection before baseline, with shared definition diagnostics once target resolution is reached; unrelated malformed-manifest errors retain their existing precedence. Confirmed new spec/report inclusion in the source inventories, retained historical metadata, and reviewed Japanese paragraphs separately from YAML/link checks. No schema change or unmeasured performance guarantee is asserted.

## Scope limitations

Validation runs on macOS through public `run_with_io` entry points and existing Rust harnesses. New tests do not claim an installed-wheel or separate native CLI subprocess check. Linux/Windows native enforcement, wheel construction/smoke and Python-only test suites were not rerun for this Rust selection change. The existing source parser and target-resolution I/O have no newly introduced global timeout; symbol files incur a second parse during later candidate discovery. Definition existence is syntactic, not Python runtime name resolution.

## Final gate results and publication reviews

`cargo test -p hoimin-cli --features contracts -- --test-threads=2` completed with exit 0 after the lint-only declaration relocation. All eight new symbol regressions ran and passed again. `cargo test -p hoimin-core --features contracts` also completed with exit 0. Both formatting checks, workspace all-target/all-feature Clippy and locked parser Clippy completed with exit 0. The earlier default-concurrency contracts timeout is not relabeled as a successful run.

1. Reviewed the PR draft against acceptance conditions and actual evidence. It distinguishes public handler tests from subprocess/wheel tests, documents extra parsing and timeout scope, and preserves the initial contracts failure plus reduced-concurrency retry.
2. Reviewed the final file inventory and diff: only selector validation, its eight tests, README, existing OKF concept/indexes and this issue's three development artifacts are included. The local `.venv` symlink and dedicated Cargo output are excluded.
3. Re-ran YAML/reserved-file validation for all 19 knowledge pages, issue-476 source hashes and matching footnotes (7 entries), local Markdown links in touched pages (616 destinations), and reachability of all 19 pages from the root index. Reviewed claims against the final source after the lint relocation; no unverified native-platform or runtime-binding guarantee was added.

The parent agent separately reviewed the code and design and reported no blocking finding: pre-intersection exact definition checks, AST depth/disposal, disambiguating file/qualname diagnostics and the documented extra-parse limitation were checked.

## Publication

Implementation commit: `9d9e79247ac4f9146c623102c7e5503bcd1e94ab`. [PR #536](https://github.com/tokyogas-tech/hoimin/pull/536) was created against `main` from `enhancement/issue-476`. The initial remote check was in progress when publication was verified; local success is not a claim that hosted CI has finished.
