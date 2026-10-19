# Issue #999: production budget proofs and final integration

Task 5 implementation, local distribution acceptance and current-source native
Linux CI are complete. Native run
[39197995202](https://github.com/HayaoSuzuki/hoimin/actions/runs/39197995202)
at `5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee` passed, including genuine cold and
warm proofs. The earlier non-Linux run
[39198000067](https://github.com/HayaoSuzuki/hoimin/actions/runs/39198000067)
failed Windows Rust tests, as did the scheduling follow-up `39199825814`.
Diagnostic run `39201076428` passed the two Job Object fixtures but exposed a
separate lifecycle adapter wait that omitted existing shutdown allowances.
The derived-bound repair passed all nine jobs in
[39202749924](https://github.com/HayaoSuzuki/hoimin/actions/runs/39202749924)
at `312be2c4759be00a26e2f04519258f1172057a67`, closing native platform acceptance.
The scoped fix review found no new breakage; its then-pending Windows gate is now
satisfied by that run. Final independent whole-branch review remains pending.
No merge, release or proof-image publication was performed.

## Source and evidence map

The approved design is [here](../specs/2026-10-19-issue-999-creusot-adoption-design.md);
maintenance commands and contract scope are in the
[proof guide](../../../formal/creusot/README.md). The
[durable archive](issue-999/README.md) preserves task briefs/reports, independent
reviews, controller Rulings, failed attempts and measurement evidence with a
SHA-256 manifest. Historical reports retain their original status wording.

| Stage | Source / result | Durable record |
| --- | --- | --- |
| Extraction | `e5ce0c6d`, independently approved | [Task 1](issue-999/records/task-1-report.md), [review](issue-999/records/task-1-review.md) |
| Pinned tools | Task 2 and effective-project-config repair approved | [Task 2](issue-999/records/task-2-report.md), [re-review](issue-999/records/task-2-fix1-review.md) |
| Production proofs | `68ea9956`, three targets × 25 files / 59 goals, five compiling sensitivity cases rejected | [Task 3](issue-999/records/task-3-report.md), [review](issue-999/records/task-3-review.md) |
| Native warm/cold | `41c888f5`, run `39191762043`, both passed | [Task 4](issue-999/records/task-4-report.md), `evidence/task-4.tar.gz` |
| Runner review fixes | `b5d920b5` then `62e3e390`, final re-review approved | [review](issue-999/records/task-4-review.md), [round 1](issue-999/records/task-4-fix1-review.md), [round 2](issue-999/records/task-4-fix2-review.md) |
| Upstream merge | `878be0d5` merges main `f4460fc0`, preserving proof-source commits | [Rulings and costs](issue-999/records/progress.md) |
| Proof-source checkpoint | `5033e1bb`, portable oracle premise and absolute Coma spans; current native warm/cold passed | [durable reviews](../reviews/2026-10-19-issue-999-creusot-adoption.md), `evidence/task-5-final-ci.tar.gz` |
| Windows harness follow-up | `70ea57b2`, two harness threads; same readiness failures persisted | [fix review](issue-999/records/task-5-fix1-review.md) |
| Diagnostic checkpoint | `dd2674dd`, both Job Object cases passed; lifecycle exit guard failed | [round 2 report](issue-999/records/task-5-fix2-report.md), `evidence/task-5-fix2.tar.gz` |
| Lifecycle guard repair | `312be2c4`, existing shutdown/telemetry allowances included; all nine native non-Linux jobs passed | [round 2 report](issue-999/records/task-5-fix2-report.md) |

The task evidence archive maps `task-N/` to the original SDD task evidence
folder. The original workspace remains intact. Final user-facing claims use
these durable paths, not an assumption that ignored scratch will survive.

## Final code repairs

The retained Windows failure (run `39192791500`, job `117869024599`) produced
zero executed/durable results for `stop_preserves_accepted`. Only Windows used
an unguarded one-second total timeout; Unix already required one persisted killed
result before applying the unchanged Lean expectation. Task 5 makes that helper
portable, with its existing 12-second total timeout, eight-second premise wait
and bounded OS-specific cleanup. It removes the unused boolean/one-second path.
Failure to establish the premise is an infrastructure error. The later passing
pre-fix Windows run is retained but does not erase this demonstrated weakness.
No model, generated corpus, expected result or production lifecycle changed.

The fixed Creusot version supports absolute spans (its default). The runner now
uses them so generated nested Coma files resolve the actual source path.
Current-lock local ARM Linux replay took 34.133 seconds; each target freshly
generated 25 files and proved 59 goals with 25 strategies and `--no-cache`.
All three proof logs had zero source-span warnings, replacing 393 per target.
Maximum measured process-or-child RSS was 272,184 KiB; the owned target directory
used 187,648 KiB and the report directory 156 KiB. Container disk afterward had
23,385,636 KiB free. Absolute paths require the original live source tree or
regeneration; old relative-span fixture logs remain unchanged.

## Current-checkpoint native proof

At `5033e1bb`, native Linux x86_64 proof job `117884864564` in run
`39197995202` passed with `cache_hit=true`: setup-inclusive 247 seconds,
runner 156.09 seconds, three targets × 25 fresh files / 59 goals / 25 strategies.
Generation peaked at 1,514,396 KiB process-or-child RSS, proof at 273,676 KiB;
output was 29,542 bytes. Absolute spans produced no source-path warnings.
The same source passed genuine cold attempt 2, job `117888001628`, after the
controller evicted only its owned branch tool-image cache. The full job log
records `Cache not found`. Setup-inclusive time was 693 seconds and runner time
105.023 seconds. Each target again freshly generated 25 files and proved all
59 goals with 25 strategies. Generation/proof peak process-or-child RSS was
1,516,044 / 273,544 KiB; the tool-build step peaked at 1,086,792 KiB. Output was
29,542 bytes with no source-span warnings. After cold completion, disk and
memory available were 81,462,580 and 14,552,420 KiB. Both jobs fit the unchanged
30-minute cap. Runner durations are observations from separate jobs; they do
not establish that a cold setup makes proving faster.

The `evidence/task-5-final-ci.tar.gz` archive contains
`task-5-final-ci/accepted-attempt-1/` and `accepted-cold/`, full job logs and
`run-attempt-{1,2}-metadata.json`. Both run attempts concluded success; attempt 2
reran the proof job and retained the other passing jobs from attempt 1. This
establishes current-lock native CI acceptance, including Rust, quality, Lean,
contracts, fuzz, random-order and wheel checks. The optional Linux cgroup-v2
hard-mode job was skipped in both attempts; it is not counted as executed.
Historical `41c888f5` results
remain separately identified. Native Windows follow-up is tracked below.

## Dependency, license and distribution audit

The prepared cargo-deny 0.20.2 binary is the official macOS ARM archive with
SHA-256 `fe67d82a10d8597a3549364cb733a3f9cc1bfff9031b7ae46384a9f2a72090c3`.
The two prior RED audits at `4137a294` rejected only `creusot-std` and
`creusot-std-proc` 0.14.0 (LGPL-2.1-or-later). Task 5 adds exact-version
exceptions, retaining the global allow-list. Both locked, unfiltered all-features
root/fuzz audits now report `licenses ok, sources ok`.

Inspection covered complete locked metadata, package manifests and target kinds,
the upstream root license, macro dispatch, `requires`/`ensures` contract-item
expansion and structural DeepModel generation, plus actual generated Coma and
the normal dependency tree. Published `creusot-std` has no packaged LICENSE;
we preserve the pinned upstream root license verbatim in
[the notice](../../../licenses/creusot-LGPL-2.1-or-later.txt). Its upstream blob is
`8000a6faacf471c537530805ab29523c7732e11a` at
`65349025c67e69fcd7479f1f420fa5904f69bc27`. Task 3's source-equality check for all
Rust files of both Creusot packages and `pearlite-syn` remains source evidence.
`pearlite-syn` declares MIT OR Apache-2.0. Neither Creusot package has a custom
build script; the proc-macro target executes during verification.

The stable math normal/build tree contains Serde and its macros only. Cfg-gated
Creusot imports, models and attributes are absent before normal macro expansion.
These graph observations, source equality and cargo-deny success are not a legal
compatibility or redistribution conclusion. The
[licensing guide](../../licensing.md#creusot-verification-dependencies) states
notice/source review conditions for upgrades, custom builds, generated proof
artifacts and proof images. Hoimin-authored sources retain ELv2.

Regenerated build-script metadata contains 213 root packages / 45 custom-build
targets and 111 fuzz packages / 21 targets. Against the dated inventory, root
Serde JSON moves to 1.0.154 and obsolete Windows 0.53.1 target entries disappear;
fuzz includes wasm-bindgen and wasm-bindgen-shared 0.2.129. The latter are
platform support in the locked graph. The historical inventory was not a fresh
baseline, so these are inventory differences, not all attributed to this task.
The separate fuzz graph retains Serde JSON 1.0.151; identical versions across
independent lockfiles are not required. Both graphs select the same 0.14.0 proof
packages and include the production math crate; neither audit rewrote a lock.

Actual local package acceptance used source/build inputs at `5033e1bb`, Rust
1.99.0, Python 3.14.7 and macOS 15.7.7 ARM64. Maturin built an sdist, then built
a locked release wheel from its extracted tree in 60 seconds. Extracted
Cargo.lock and all three math Rust modules were byte-identical to the checkout.
The annotated crate's manifest and sources are present; registry proof crates
are lock references rather than vendored source. Normal build needs no proof
tools. The isolated wheel smoke passed metadata/help/version checks and actual
mutation execution with original-source integrity checks.

| Local artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `hoimin-0.13.0.tar.gz` | 1,387,284 | `cb0cadd2ab4686a0b0a105f9349ff39e9fdeac74d69be7ff4ef6430b78a22c27` |
| `hoimin-0.13.0-py3-none-macosx_11_0_arm64.whl` | 5,854,585 | `765c0e053ca307b5c5e05c6f0841e9dd16fedba9148a8e273444666ca1097c99` |

Pinned cargo-cyclonedx 0.5.7 generated the actual extracted-tree shipping graph:
149 components, one matching-version math/core/Python library each, and no
`creusot-std`, `creusot-std-proc` or `pearlite-syn`. The raw CycloneDX schema and
repository finalizer/offline release validator passed against the wheel hash.
This is a local release-policy dry run: the finalizer's `macos-14` field is its
configured release environment label, not observed host provenance. The separate
smoke/provenance JSON records the actual macOS 15.7.7 host. An initial helper call
used unsupported `macos-arm64`, raised `KeyError`, and produced no validated
output; rerunning with the repository's `macos-aarch64` identifier passed.
Nothing was uploaded or published.

## Reproduction commands and results

Run from the issue worktree. Use the task-owned paths below; the worktree `.venv`
links to the isolated environment. Main's `.venv` target was not modified.

```sh
export CARGO_TARGET_DIR=/tmp/hoimin-issue999-target CARGO_BUILD_JOBS=2
export UV_CACHE_DIR=/tmp/hoimin-issue999-uv-cache
UV_PROJECT_ENVIRONMENT=/tmp/hoimin-issue999-task5-venv uv sync --frozen --group fuzz
cargo fmt --all -- --check
cargo test --locked -p hoimin-cli --test lean_result_lifecycle_oracle -- --nocapture
/tmp/hoimin-issue999-task5-venv/bin/python -m pytest -q tests/test_verify_creusot.py
```

Results: controlled environment sync passed; six lifecycle tests passed,
including the real durable-result premise and bounded descendant cleanup;
runner tests passed 39 with three Linux-only skips on the host. The actual Linux
proof runner passed on pinned Python 3.12.3, separately from host unit fixtures.
The Task 4 Linux signal-delivery and parent-cleanup results remain archived.

```sh
# The prepared directory supplies cargo-deny 0.20.2 without a system install.
export PATH=/tmp/hoimin-issue999-audit-tools/cargo-deny-0.20.2/unpacked/cargo-deny-0.20.2-aarch64-apple-darwin:$PATH
cargo deny --manifest-path Cargo.toml --workspace --all-features --locked --config deny.toml check --deny warnings licenses sources
cargo deny --manifest-path fuzz/Cargo.toml --workspace --all-features --locked --config deny.toml check --deny warnings licenses sources
cargo metadata --locked --all-features --format-version 1
cargo metadata --manifest-path fuzz/Cargo.toml --locked --all-features --format-version 1
cargo tree -p hoimin-budget-math --locked --all-features --edges normal,build
```

Both audits passed. Metadata and normal-tree logs accompany the inventory.

```sh
cargo install cargo-cyclonedx --version 0.5.7 --locked --root /tmp/hoimin-issue999-audit-tools/cyclonedx
/tmp/hoimin-issue999-task5-venv/bin/python -m maturin sdist --out /tmp/hoimin-issue999-package/sdist
# Extract with Python tarfile.extractall(..., filter='data') into the owned source directory.
/tmp/hoimin-issue999-task5-venv/bin/python -m maturin build --release --locked --manifest-path /tmp/hoimin-issue999-package/source/hoimin-0.13.0/crates/hoimin-cli/Cargo.toml --out /tmp/hoimin-issue999-package/wheels
PATH=/tmp/hoimin-issue999-audit-tools/cyclonedx/bin:$PATH /tmp/hoimin-issue999-task5-venv/bin/python tools/capture_sbom.py --root /tmp/hoimin-issue999-package/source/hoimin-0.13.0 --target aarch64-apple-darwin --output RAW_JSON
HOIMIN_WHEEL=/tmp/hoimin-issue999-package/wheels/hoimin-0.13.0-py3-none-macosx_11_0_arm64.whl /tmp/hoimin-issue999-task5-venv/bin/python tests/wheel_smoke.py
/tmp/hoimin-issue999-task5-venv/bin/python tools/sbom.py --root /tmp/hoimin-issue999-package/source/hoimin-0.13.0 --raw RAW_JSON --directory /tmp/hoimin-issue999-package/wheels --platform macos-aarch64 --kind wheel --version 0.13.0 --commit 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
/tmp/hoimin-issue999-task5-venv/bin/python -m pytest -q tests/test_sdist.py tests/test_release.py tests/test_release_version_contracts.py tests/test_sbom.py tests/test_sbom_properties.py tests/test_sbom_oracle.py tests/test_docs_site.py
```

The focused distribution/SBOM/docs suite passed 399 tests in 17.38 seconds with
updated Hypothesis 6.170.0. Actual build/smoke and raw/finalized SBOMs are separate
evidence from these tests. Local host free disk moved from 98 to 96 GiB; the
shared task target used 19 GiB, isolated environment 353 MiB and package work
15 MiB. Only one heavy local build ran at a time. No unrelated container,
worktree, cache or SDD evidence was deleted.

Full Rust, all-features Clippy/contracts, Python quality, Lean build/corpus
freshness/correspondence, fuzz and native platform regression use the controller's
same-source CI runs above. This avoids repeating the already accepted Task 3
suite locally. Task 3's 2,916 passed / zero failed / 33 ignored stable workspace
result remains historical evidence, not a claim about execution after the merge.

## Final local document and quality checks

`cargo fmt --all -- --check`, `git diff --check`, `ruff check .`,
`ruff format --check .` and `ty check` passed. Safe YAML parsing validated all
46 OKF concepts and four reserved files. Current-source fingerprints, changed-page
file links and footnote definitions passed; remote URLs and heading anchors were
not fetched. The first link check exposed the existing removed analyzer/rust.rs
reference; it now points to the actual hoimin-python API, with refreshed source
metadata. Historical source entries outside this update retain their hashes.
The original archive SHA-256 and six-Ruling preservation checks passed. The final
archive also preserves the seventh scheduling Ruling and its recorded costs.

Exact local proof execution after copying the checkpoint runner and merged lock
into the owned container was:

```sh
docker exec hoimin-issue999-task3 bash -c 'cd /workspace && python3 tools/verify_creusot.py --output /tmp/task5-absolute-span && df -k /workspace && du -sk /tmp/hoimin-issue999-target /tmp/task5-absolute-span'
```

The archived `task-5/absolute-span/summary.json` records every subprocess argv,
per-target timing/RSS and result. `task-5/validate-okf.py` contains the actual
safe-YAML/link/fingerprint validator; `task-5/okf-validation.json` records its
scope and counts. `task-5/package-provenance.json`, raw/finalized SBOM, smoke JSON,
audit logs and metadata record local distribution evidence. Run archives preserve
output, not the 5.9-MB wheel; its exact hash and rebuild inputs are recorded above.

## Five implementation self-reviews

1. **Requirements and public domain:** checked all eight arithmetic requirements,
   Q1–Q7 and separate target generation against the proof guide, manifest and
   Task 3 review. Task 5 changes no math, input type, serialized representation,
   resource cap, tool pin or warning-only policy.
2. **Design and ownership:** the Windows repair reuses the existing portable
   persistence check and OS cleanup, removing obsolete configuration. The span
   repair uses one pinned option. Against the eight software-design diagnostics,
   seven rows pass (8.75/10): responsibilities remain clear, interfaces hide
   mechanics, no new abstraction or caller coupling appears, comments state the
   semantic premise, and review includes maintenance cost. The eighth row's
   10–20% design-effort allocation was not measured; reaching 10/10 would require
   that measurement, not another code abstraction.
3. **Distribution and licensing:** inspected both complete graphs, proc-macro
   selection/expansion, upstream license and the actual sdist/wheel/SBOM. Kept
   exact-version exceptions and distinguished package absence in normal builds
   from legal compliance. Preserved source links and the upstream notice.
4. **Documentation and evidence:** retained upstream C4 changes while adding the
   math boundary; documented full-domain guarantees, exclusions, updates, repair
   commands, source/spec size, native cold/warm costs and sensitivity. Preserved
   failed attempts, original commits and all controller Rulings durably.
5. **Scope and operational safety:** checked the final code diff, task-owned
   environments, resource observations and archive manifest. No shared main
   Python environment, remote settings, release or image publication changed.
   Controller owns pushes, final reviews and PR acceptance.

## Five validation self-reviews

1. **RED and correspondence:** reused actual Windows mismatch and both prepared
   license RED logs. Kept the original Lean expectation and distinguished a
   missing execution premise from semantic mismatch. The historical source-span
   warnings are preserved as the diagnostic baseline.
2. **Focused behavior:** verified six lifecycle tests, 39 runner tests and real
   three-target proof execution. The Linux-only host skips are explicit. Native
   Windows acceptance is reported by its actual CI run, not inferred from Unix.
3. **Distribution evidence:** compared extracted source bytes, built the wheel
   from those sources, ran isolated CLI mutation smoke, and validated actual
   generated graph/schema/artifact binding. The 399 focused tests complement
   those artifacts. Invalid local platform naming is recorded as a helper error.
4. **Integration and resource claims:** retained older accepted suites with their
   revisions; assigned changed-lock full regressions to current-source CI.
   Cold/warm native measurements stay distinct from preinstalled ARM development
   runs. RSS, address space, disk and elapsed time have explicit measurement scope.
5. **Document validation:** parse OKF with safe YAML, check its reserved index,
   source fingerprints, changed-page links and required footnotes separately
   from prose review. Preserve historical source hashes outside the updated
   concepts; source inspection never becomes a fabricated fresh execution claim.


## Follow-up: Windows workspace harness scheduling

The local Task 5 scoped review approved `878be0d5..46ef6b9e` and identified two
Minor documentation findings, corrected in this follow-up. The complete native
CI run `39197995202` attempt 1 passed at `5033e1bb`. The same checkpoint's
non-Linux run `39198000067` failed two pre-existing Windows Job Object fixture
cases before readiness/identity publication; macOS Rust and the Windows resource
slice passed. The failed Windows library binary reported 761 passed, two failed
and 23 ignored. This was not a result-lifecycle oracle mismatch.

The [fix report](issue-999/records/task-5-fix1-report.md) records the evidence and
five self-reviews. The existing Issue #967 record observed the same two bounded
tests fail under parallel execution (one at a different phase), then pass with
two threads. This supported a contention hypothesis, not a confirmed cause. The
manual Windows full workspace suite now uses `--test-threads=2`; that scheduling
change preserved production/fixture code, every test and each six-second bound.
However, follow-up run `39199825814` at `70ea57b2` failed the same two readiness
cases: 761 passed, two failed, 23 ignored in 57.69 seconds. The mitigation was
insufficient. The seventh Ruling and its longer-CI/remaining-cause risk remain in
the archive, together with the contrary result. The scoped fix review found both
Minor findings addressed and no new breakage, but left native acceptance open.

Local regression: the new Windows workflow case failed before the scheduling
change, with its macOS control passing. Final workflow/docs tests passed 194
cases; Ruff and ty passed. The two source-provenance/count findings are corrected.
The focused follow-up archive contains these results and the scoped review.

## Diagnostic follow-up and lifecycle exit guard

The [round 2 report](issue-999/records/task-5-fix2-report.md) records checkpoint
`dd2674dd` and diagnostic run `39201076428`. Eight jobs passed, including native
Windows Clippy. Both six-second Job Object cases passed in the full Rust job and
the separate resource job. The latter recorded completed import/spawn/readiness
phases and five `ResumeThread` calls with prior suspend count 1. This single run
did not establish why earlier readiness failed or prove scheduling fixed it.

The full Rust job instead failed `stop_preserves_accepted` with an infrastructure
error while waiting for CLI exit. Concurrent pipe readers were already active.
Its captured 5,950-byte JSON reported one killed result, one not_run result and
exit code 4, with empty stderr; emitting that report did not prove process exit.
The adapter waited only 12 seconds after observing the durable premise, although
the configured 12-second execution timeout is followed by the shell's documented
two-second shutdown grace and the binary's bounded 250-ms telemetry flush.

Checkpoint `312be2c4` derives the exit wait from those existing allowances:
14.25 seconds after observing the premise. The separate eight-second premise wait
and bounded failure cleanup remain unchanged; 14.25 seconds is not a whole-process
wall-clock cap. Production timeouts, the Lean corpus and expected outcome, and
all six-second Job Object bounds remain unchanged. Three paused-time boundary
tests failed on the old guard. All nine lifecycle tests then passed locally in
13.52 seconds, including real strict correspondence and descendant cleanup;
focused Clippy passed in 7.33 seconds. Native run `39202749924` subsequently
passed all nine jobs. The [scoped review](issue-999/records/task-5-fix2-review.md)
found the guard and local checks sound, with no new breakage. It preceded the
native result and retains its original then-pending acceptance finding. Its RED
evidence limitation is explicit: the exact command/result summary survives, but
the raw RED terminal transcript was not archived.

The Windows diagnostics are test-only and bounded: whitelisted Python environment
values, limited file reads, interpreter/cwd, import/publication phases, native job
limits and thread resumption. They add no product diagnostic flow. Their
maintenance cost is explicit: additional fixture I/O can perturb startup timing,
and internal test logging adds noise. The intermittent readiness cause remains
unproven. The bounded diagnostics are retained as failure context for these native tests;
they are not a claimed repair for the intermittent readiness failure. Final
whole-branch review can assess their maintenance cost alongside the evidence.

## Final native acceptance and remaining review

Run `39202749924` at `312be2c4` completed successfully, with all nine jobs
successful. The archived `non-linux-final-312be-metadata.json` and raw logs record:

| Native check | Observed result |
| --- | --- |
| Windows Rust, job `117899018511` | 173 suite summaries: 2,886 passed, zero failed, 31 ignored; 20m18s job duration within the 30-minute cap |
| macOS Rust, job `117899018190` | 172 suite summaries: 2,931 passed, zero failed, 33 ignored |
| Windows wheel, job `117899018493` | 1,155 passed, 51 skipped in 94.82s |
| macOS wheel, job `117899018525` | 1,192 passed, 14 skipped in 67.49s |
| Windows resource scope, job `117899018476` | 37 passed across three suite summaries |
| Remaining native jobs | Both quality jobs, Windows dependency purity and Windows metrics destinations passed |

Rust counts aggregate the commands in each job, not just one workspace invocation.
The Windows log includes both six-second Job Object tests, all three new timeout
boundary tests and real `result_lifecycle_oracle_correspondence` passing. This
closes the scoped review's external acceptance gap; it does not explain the prior
intermittent readiness failures. Failed runs and their diagnostic limitations are
preserved above and in the archive.

Native Linux full CI and warm/cold proof remain sourced at `5033e1bb`. Production
math, root dependency inputs, proof targets/strategies, runner and tool recipe are
unchanged through `312be2c4`; only the proof guide changed within those paths.
Later native macOS/Windows execution covers the fixture/adapter changes. The final
PR will run its ordinary fresh proof job; an identical extra cold tool build is
not claimed or required as new evidence here.

The [final acceptance report](issue-999/records/task-5-final-acceptance-report.md)
records the focused documentation/OKF/archive checks and five final self-reviews.
All seven Rulings and their costs, both scoped fix reviews, complete current CI
metadata and retained raw logs are archived. Task 5's implementation acceptance
is complete; final whole-branch review, any resulting repair, and PR publication
remain controller-owned work. No merge or release was performed.
