# Strict Python quality tooling design and implementation plan

## Design

Base: `9c2f7f5`. Add dev-only `ruff>=0.16.1,<0.17` and `pytest>=9,<10`, retaining
existing development dependencies and the binary wheel's empty runtime dependency
set. Use the reference kraken-hub configuration's Ruff ALL selection, Python 3.14,
88-column formatter, and its applicable style exclusions. Do not copy Django,
coverage/reporting plugins or type-checker configuration unrelated to this task.
Checks must not edit files; formatting and fixes are explicit developer commands.

Maintain current unittest tests and run them through pytest with strict mode,
strict collection of empty parameter sets, and `testpaths = ["tests"]`. Exclude
fixture projects from recursive test discovery. Ruff covers maintained tests,
Python tools, the Lean resource guard and Windows helper; exclude vendor code,
historical docs/audits, generated build output, worktrees and mutation fixtures.
Use scoped exceptions for script modules and existing unittest assertions; keep
all other ALL rules enabled and fix violations rather than disabling families.
Explain local suppressions for required subprocess/CLI behavior and deliberately
complex existing orchestration. Preserve embedded fixture source and shell text.

CI: run read-only Ruff format/lint in the automatic quality job after a frozen
no-project dev sync; do not force a Rust build just to lint. Use pytest for the
existing Python suite in wheel smoke (automatic and manual), preserving ordering,
Linux-only automatic execution, pinned actions and artifact-only releases.

## Implementation plan

1. Baseline existing unittest suite and inventory Ruff violations using the
   reference policy. Commit this design and plan before code changes.
2. Add dependencies/configuration and regenerate uv.lock. Run baseline pytest,
   collect the same existing tests, and record initial lint failures.
3. Format and fix maintained Python files in independent groups; retain test
   meaning and tool behavior. Do not rewrite unittest tests just for pytest style.
4. Update workflow commands and their contract tests; add checks for quality
   execution, dev-only dependencies, strict discovery and meaningful Ruff policy.
   Validate failing quality settings before fixing them, then run all pytest tests.
5. Update developer commands and reproducibility design/OKF source provenance.
   Run frozen sync/lock validation, Ruff lint/format, pytest, and actual wheel smoke
   if its helper changes. Use focused hoimin checks for substantive Python tool
   behavior changes; formatting-only changes do not need mutation verification.
6. Complete three implementation and test reviews plus an independent whole-diff
   review. Commit and create a PR, preserving the worktree and main checkout.

## Design self-reviews

1. Scope: the requested strictness means ALL plus justified exclusions, not just
   Ruff's default E/F set. Restrict exclusions to historical data or documented
   script/unittest conventions rather than excluding maintained tool code.
2. Behavior: pytest can collect unittest without assertion conversion; fixture
   tests must not be collected as project tests. Lint checks must be read-only.
3. Integration: dev-only dependencies must not appear in built wheel metadata.
   Existing platform policy, resource checks, scripts and release guards stay.

## Plan self-reviews

1. Baseline evidence: all 103 unittest tests passed before changes. Initial Ruff
   inventory had 1,487 diagnostics, mostly quotes, unittest style and formatting;
   fixture diagnostics are outside the maintained-source scope.
2. Change isolation: delegate disjoint Python files; root owns shared config,
   CI contracts and docs. Test shared behavior after integration, not only each
   agent's individual files. Never blanket-apply unsafe fixes.
3. Verification: use frozen dev dependencies and the actual root commands;
   check collection and unchanged subprocess/fixture behavior. Document any
   additional scoped ignore so the requested strictness remains reviewable.

## Implementation self-reviews

1. Configuration: reviewed the resolved lock diff and reference policy. Only Ruff,
   pytest and their missing transitive dependencies were added; existing versions
   stayed unchanged. Ruff 0.16.8 / pytest 9.1.1 are locked. ALL remains enabled;
   only standalone-module and legacy unittest conventions receive extra scoped
   exceptions. Ruff discovery includes all 12 maintained Python modules.
2. Behavioral preservation: reviewed annotations, imports and equivalent style
   changes across the tools. Differential generation for 29 fixture shapes at
   sizes 1/2/4 produced 87 identical file/metadata pairs. Existing assertion ASTs
   and embedded strings were preserved. Process cleanup, exit classification,
   Windows ctypes layouts and source fixtures retain their semantics. No new
   production behavior justified a mutation-testing campaign.
3. Integration: automatic/manual quality steps remain equal and Python tests
   still precede wheel reset/build/smoke. Lint runs with `--no-fix` and format
   with `--check`; dev-only sync avoids a build for these checks. The Lean
   boundary job deliberately retains standard-library unittest. Rust sources,
   platform triggers, compiler pins and artifact-only release rules are unchanged.

## Test self-reviews

1. Collection and regression: pytest initially collected all 103 existing tests.
   The new workflow contract failed before the CI changes, including both missing
   quality setups and the old test command/documentation. After integration,
   all 104 tests and 227 subtests passed under Python 3.14.7. Process-inspection
   tests require normal `ps` access; restricted-sandbox failures disappeared
   under the approved test execution environment.
2. Strictness and read-only checks: temporary probes confirmed F821 and formatting
   violations fail without changing source bytes; explicit fixture paths are
   excluded. Pytest excludes fixture projects and rejects unknown markers,
   unknown configuration keys and empty parameter sets. These probes used the
   actual copied project configuration and locked tool executables.
3. Packaging: a fresh macOS ARM64 release wheel built successfully with maturin
   1.15.0 (`--release --locked --compatibility pypi --no-default-features`). The
   updated wheel smoke script passed against that wheel, including metadata
   checks for absent runtime dependencies and isolated installation/execution.
   This does not claim native Windows execution of the ctypes helper.

## Final verification and independent review

- `uv lock --check` and `uv sync --frozen --no-install-project`: passed.
- `uv run --frozen --no-sync ruff format --check .`: passed.
- `uv run --frozen --no-sync ruff check --no-fix .`: passed.
- `uv run --frozen --no-sync pytest -q`: 104 passed, 227 subtests passed.
- Fresh release wheel build and `python tests/wheel_smoke.py`: both exit 0.
- `git diff --check`: passed.
- OKF: 25 YAML concept headers/four reserved indexes checked; catalog source IDs,
  local paths and footnotes checked; amended design's SHA-256 matches. Only the
  relevant source record was refreshed; historical audit evidence was retained.
- Independent whole-diff review: no findings. The reviewer checked code/config,
  CI consistency, Windows version compatibility and source coverage, and ran the
  new workflow contract test (one test/two subtests). Parent verification covers
  the full suite, wheel, fixture comparisons and documentation checks above.

Design and plan were committed before implementation in `09e766b`. No native
Windows/macOS CI workflow was dispatched and no PR was merged automatically.

The exact CI invocation `uv run --frozen pytest -q` also passed all 104 tests and
227 subtests, including uv's editable project build/synchronization.

## Follow-up design: pytest-cov and pytest-randomly

The requested plugins extend this still-open PR. Add dev-only
`pytest-cov>=7,<8` and `pytest-randomly>=4,<5`, matching the reference project's
major-version policy, and refresh the lock without upgrading unrelated packages.
pytest-randomly automatically shuffles existing tests. Keep coverage opt-in and
show an explicit branch-coverage command for `tools/`; no arbitrary percentage
threshold, subprocess instrumentation or additional CI jobs are introduced.
Document seed reproduction and ignore generated coverage artifacts.

### Follow-up plan

1. Commit this design/plan, then add both dev dependencies and regenerate the lock.
2. Run the full suite with two recorded seeds, one with branch coverage; verify
   randomized collection is repeatable for one seed and differs for another.
3. Check Ruff/format/frozen lock, update the developer guide and PR description,
   commit and push to the existing branch.

### Follow-up design self-reviews

1. Scope: dev-only plugins preserve the executable wheel's dependency boundary.
2. Reproduction: enable plugin defaults and document numeric/last seed replay;
   no fixed default seed hides order dependencies.
3. Evidence: the explicit coverage command measures Python tools only, not Rust
   code or every subprocess; avoid presenting that report as whole-product coverage.

### Follow-up plan self-reviews

1. Keep the existing open PR/worktree and preserve unrelated locked versions.
2. Multiple seeds and collection comparisons exercise actual plugin integration.
3. Coverage output remains local/ignored; installing the plugins does not silently
   add a threshold or change the automatic platform execution policy.

### Follow-up implementation self-reviews

1. Dependency boundary: only pytest-cov 7.1.0, pytest-randomly 4.1.0 and coverage
   7.16.1 were added to the lock. All previous package versions remain unchanged;
   the project runtime dependency list remains empty.
2. Reproduction: removed quiet flags from automatic/manual CI pytest commands
   and corresponding contract expectations so every run prints its random seed.
   Developer commands likewise show the header. No default fixed seed was added.
3. Output and scope: coverage is explicitly requested for tools only, generated
   data/report paths are ignored, and no threshold or subprocess patch is enabled.
   Updated the related design source and verified its catalog revision/link/hash.

### Follow-up test self-reviews

1. Full suite: seed 12345 with coverage passed all 104 tests; seed 67890 without
   coverage also passed all 104 tests. Both plugin names/versions and seed appear
   in session headers. The tools-only branch report totals 75% (423 statements,
   178 branches); it is not product-wide or subprocess coverage.
2. Order behavior: three collection runs with seeds 12345/67890/12345 contained
   the same 104 test IDs. The first and third order matched exactly; the second
   differed. No tests disappeared under randomization.
3. Integration: uv lock check, Ruff lint/format and diff whitespace checks passed.
   Git ignores all four documented coverage output patterns. Production code and
   wheel build metadata are unchanged, so a repeated wheel build was unnecessary.

Incremental independent review found no blocking issues. Its suggestion to show
seed output in the Python-only developer command was applied. Validation above
belongs to this follow-up, distinct from the earlier wheel/fixture evidence.
