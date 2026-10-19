# Issue 999 Creusot adoption reviews

## Task 1: production arithmetic extraction

Scope: `hoimin-budget-math` extraction and integration, before proof annotations or proof-tool installation. Reviewed against the approved design and Task 1 brief; base commit `7110f2ef`.

### Implementation self-reviews

1. **Production ownership and dependency direction.** Confirmed duration types, auto timeout and projection arithmetic reside only in the new crate. Core configuration, machine and resume code import the owner directly. Removed the old core projection module and its exports; CLI's timeout assertion imports the new owner through its own direct dependency. No compatibility functions, core type reexports or copied arithmetic remain.
2. **Serde feature and invariants.** Compared the old constructor/deserializer with the extraction. The public constructor returns `Option`; every core conversion maps `None` to the same named `ConfigError::InvalidLimit`. Preserved the deserialization diagnostic `invalid zero or overflowing limit: duration`, private tuple field, snake_case enum representation and Duration representation. Serde only gates trait implementations/attributes; value types and arithmetic branches are identical with the feature disabled.
3. **Arithmetic and public input domain.** Compared the moved projection and auto-timeout bodies with the base versions. They retain full `usize`, `NonZeroUsize`, Duration and positive fixed-duration domains, overflow handling, nanosecond reconstruction and warning-only semantics. `is_shortfall` still reads only capacity and remaining, including for manually constructed public structs. No configuration ceilings were introduced into the arithmetic crate.
4. **Crate graph and distribution.** Checked workspace membership, both lockfiles, CLI/core dependencies, release version rewriting, SBOM required members and sidecar cleanup. Found the explicit cleanup list in `tools/capture_sbom.py` and updated it after the new cleanup fixture failed. Removed unrelated zerocopy upgrades introduced by fuzz lock regeneration. Added the same narrow 0.13.x Elastic-2.0 license exception as the existing internal crates; cargo-deny is not installed locally, so final dependency policy execution remains a later task. Actual sdist tests confirm Cargo manifests and all production source inputs are included automatically, so no redundant maturin include entries are needed.
5. **Change scope and stable build.** Reviewed changed-file diff, formatting, direct use sites and all-features compilation. No Creusot dependency, annotation, tool installation, verification substitute or configuration-dependent arithmetic has been introduced. Only integration consumers/tests and release/SBOM assumptions accompany the extraction. Verification results and any remaining environment limitations are recorded in the Task 1 report.

### Testing self-reviews

1. **Baseline and RED evidence.** Corrected the brief's nonexistent `config` test target to `config_json`, recorded the initial command failure, then obtained a passing pre-edit baseline (133 tests). The new crate's API tests initially failed to compile because the requested API did not exist; this is structural RED, not a claimed behavioral arithmetic failure. Python release-version and SBOM rejection tests provided behavioral RED (four failures) before their production changes.
2. **Independent arithmetic expectations.** Reviewed each property: ceiling now uses the minimal-covering-wave inequalities with widened u128 products (see the parent-review correction below); auto uses bounded u128 natural arithmetic and a literal maximum; capacity computes its wave count by a widened additive ceiling and divides the maximum by positive nanoseconds before multiplying, avoiding oracle overflow; shortfall compares independent nanosecond values. Properties check one behavior per execution and do not call production helpers for expected values.
3. **Boundary coverage and AAA.** Reviewed named macro-generated cases for zero selected, exact/remainder waves, usize maximum, >u32 waves, one nanosecond, Duration maximum, nanosecond carry, floor equality, multiplication/addition saturation and manually constructed inconsistent projection fields. Each named boundary is its own test execution; properties preserve arrange/action/assert separation. Existing configuration and Lean regression tests continue to cover consumer behavior.
4. **Serialization and packaging failures.** Corrected the new serde test expectation to the original error text before implementing the deserializer. The full Python suite also found fixed SBOM permutation counts; expanded these to five components and six dependency nodes. Initial fixture expansion shifted positional corruption-test indexes; fixed this by appending the new component and node, preserving existing fixture positions. The next focused run passed all 124 tests, including real maturin sdist inspection and both SBOM cleanup paths. Initial uv-cache permission failures were resolved using task-owned `/tmp` cache and the shared environment's Python on PATH.
5. **Feature and regression verification.** Ran the new arithmetic tests without default features to check that optional serde is unnecessary, and with serde to check compatibility. Ran focused core/Lean/contracts tests and all-features workspace checking, then the complete Rust/Python suites and normal hooks. The full Python suite passed 1100 tests with 11 skips after access to `ps` was allowed. The targeted SBOM cleanup mutation was killed with a fresh passing baseline (15,766 ms), mutant exit 1 (14,840 ms), zero survivors and complete/clean execution. The first mutation attempt stopped safely because a concurrent edit to `deny.toml` changed an original input; replanned after freezing edits and cleaned the exact temporary directory. The complete all-features Rust workspace suite passed 2916 tests, failed zero and ignored 33 (171 per-binary/doctest summaries). Record hook evidence in the Task 1 report; no proof or cross-target success is inferred from these runtime checks.

### Parent review correction: independent wave expectations

The parent's final Task 1 review identified that the initial plan requested a quotient/remainder wave oracle structurally matching production. Updated the plan as requested and replaced that property with the minimal-covering conditions `selected <= waves * jobs` and, for positive waves, `(waves - 1) * jobs < selected`, using u128 products. The independent capacity oracle now uses widened additive ceiling `(selected as u128 + jobs as u128 - 1) / jobs as u128`, then guards multiplication by division. This changes test expectations only; production arithmetic and input domains remain unchanged. Focused new-crate tests, formatting and Clippy are rerun and recorded in the Task 1 report; the previously passing full workspace/Python/mutation runs are preserved without redundant reruns.

Follow-up verification: all 35 new-crate tests passed. Clippy identified the deliberately explicit additive-ceiling expression as `manual_div_ceil`; retained the required independent mathematical oracle with a statement-local allowance and explanatory comment. Rechecked tests, formatting and focused Clippy; final command results are in the Task 1 report.

### Independent Task 1 review

Reviewed commits `7110f2ef..e5ce0c6d`: spec compliant, task quality approved, no Critical/Important/Minor findings. Review checked the real owner/API, unchanged projection and timeout arithmetic, Serde/error preservation, independent numerical expectations, both lockfiles, release/SBOM/actual sdist integration and recorded test evidence. It also searched integration locations for stale references to the removed core API. Passing tests were not rerun by the reviewer.

The controller confirmed active `rustc 1.99.0 (b940084d7 2026-09-28)` and `cargo 1.99.0 (5f94df478 2026-08-27)` in this worktree. Actual cargo-deny execution remains a final acceptance gate after adding Creusot dependencies; a narrow policy change alone is not execution evidence. Proof tools, mathematical proofs, cross-target and CI claims remain outside this completed extraction task.

Controller plan correction: the original quotient/remainder test expectation mirrored production. The plan and tests now use minimal covering waves and widened additive ceiling, without changing the accepted mathematical contract. If that choice is wrong, the tests may reject correct arithmetic or need target-width adjustments.

## Task 2: pinned verification environment

The environment task preserves the approved versions and all three proof targets.
The controller explicitly approved native Linux ARM64 for local development;
actual native Linux x86_64 cold/warm and whole-job 30-minute acceptance remain
mandatory in Task 4. The earlier emulated amd64 setup was interrupted after
624.456 seconds (exit 130) during rustc-dev installation, with no cap relaxation.

### Implementation self-reviews

1. **Pinned provenance and trusted code.** Resolved Creusot v0.14.0 to
   `65349025c67e69fcd7479f1f420fa5904f69bc27`; preserved approved Why3/Why3find
   commits. Recorded SHA-256 for official source archives, dated Rust components,
   all required target libraries and official Z3/CVC5 distributions. The opam
   repository is a fixed, hashed source snapshot; OCaml 5.3.0's exact archive
   checksum and the authoritative package recipe are recorded. No additional
   solver or custom logical trusted/assume specification is introduced.
2. **Runtime selection and drift.** Preflight checks every required executable's
   installed content, execute permission, actual PATH resolution, version process
   status and exact first version line. It checks the reviewed configuration
   hashes, configuration-selecting environment variables and build-receipt
   manifest digest. Version output alone cannot validate changed binaries or a
   missing solver. Receipts close drift checks for built executables and target
   standard libraries; they do not authenticate a maliciously built image.
3. **Effective prover configuration.** Traced cargo-creusot's actual WHY3CONFIG
   override into Why3find and Why3's read_config_rc. Corrected the effective
   creusot_why3.conf to include main limits and exact prover version fields;
   validating an unused user file would not establish the proof configuration.
   Kept only Z3/CVC5 selections. Documented the necessary explicit `-j 2` and
   Why3find's calibrated-time multiplier; Task 4 must independently enforce the
   actual per-process 30-second/2-GiB limits and whole-job deadline.
4. **Isolation, cache and disk.** Installation remains inside task-owned Docker
   layers under /opt/creusot; unrelated images, containers and volumes are
   preserved. Fixed artifact inputs are separated from final configuration and
   checker COPY layers. Removed obsolete downloads/extracted Rust components
   after installation. Measured Docker VM headroom as well as host free space;
   the VM initially had 32 GiB free with 13 unrelated running containers.
5. **Architecture and claim boundaries.** The pinned x86_64 recipe remains the
   acceptance baseline. Added an official ARM64 host-artifact manifest for native
   local development, with identical source commits, types, proof targets and
   configuration. Real rustc metadata invocations diagnose all three target
   standard libraries. No emulated or ARM64 timing is represented as native
   x86_64 CI timing; arithmetic proofs and mandatory CI remain later tasks.

### Testing self-reviews

1. **Observed RED before code.** The first 15 CLI tests failed because the checker
   did not exist. Subsequent behavioral RED exposed boolean-as-schema-version
   acceptance, relative tool/receipt paths, environment configuration drift and
   PATH selecting another executable; each was fixed and rerun. Controlled real
   executable files test the process/file boundary without assuming internal
   command order.
2. **Failure-path independence.** Parameterized missing tools, wrong versions and
   malformed manifest fields. Wrong-version fixtures update their receipt hash
   so the version assertion is tested independently of hash drift. Changed binary
   content prints the correct version to test the converse. A nonzero tool
   prints the correct version to distinguish process status from stdout.
3. **Mutation evidence and cleanup.** The targeted OR-to-AND mutation of the
   version failure predicate was killed with a fresh passing baseline (5858 ms),
   mutant exit 1 (6.231 s), zero survivors, complete execution and clean resource
   cleanup. The first verify invocation stopped before baseline because a relative
   linked .venv was excluded from worker copies; replanned with the absolute
   shared interpreter after clean cleanup, preserving one worker and the
   8-GiB/10-GiB disk limits. No survivor claim is based on that stopped run.
4. **Suite and tooling evidence.** Focused checker tests reached 30 passing
   cases; Ruff formatting/lint and ty passed. Full Python suite initially exposed
   four existing resource-guard ps sandbox failures and two uv-cache permission
   failures; rerun with process-inspection access, task-owned UV_CACHE_DIR and
   venv PATH passed 1130 tests with 11 skips in 41.01 seconds. Shell syntax and
   embedded Python syntax checks cover installer/configuration scripts.
5. **Actual environment evidence remains distinct.** The initial minimal Ubuntu
   apt bootstrap failed without CA roots. A dated checksum-verified CA bundle
   retained TLS verification; diagnostics then found ADD had created parent
   directories without traversal bits for _apt. Fixed directory modes and
   required any apt index failure to stop installation. Subsequent signed snapshot
   setup succeeded. Actual native build/preflight/smoke results, limits and any
   outstanding acceptance gates are recorded in the Task 2 report rather than
   inferred from the unit tests.

### Follow-up environment and resource reviews

- The controller approved native ARM64 development while keeping native x86_64
  CI acceptance unchanged. The native first attempt reached all three real
  metadata compilations and built OCaml 5.3.0, then stopped safely at opam's
  missing-checksum rejection for a local directory pin (324.107 seconds overall,
  tool-stage peak process RSS 521824 KiB). Replaced directory pins with package
  recipes in a local repository whose source archives carry the already verified
  SHA-256 values. OPAMREQUIRECHECKSUMS stays enabled; no network checksum bypass
  was introduced. Separated Rust/components, OCaml and proof-tool cache layers.
- Verified the actual pinned Linux GNU timeout 9.4 and prlimit 2.39.3 flags, then
  added both as mandatory hashed/version-checked preflight tools (32 unit cases
  pass after observed RED). The exact 30-second process-group fixture returned
  137 and its sleeping descendant was gone. The real AS2GiB setting reported
  2097152 KiB; a 1MiB AS fixture exited127 on loading libc, without a large
  allocation. Effective solver commands use absolute timeout/prlimit paths. AS
  is stricter than RSS, and no whole-environment RSS bound is asserted.
- After the PATH/environment refactor, reran the selected version-predicate
  mutation against the final helper: candidate
  `m1_ae5d015d3b22013fd7a06936fcb64d5748670daeace209003dd48b6d64acb466`,
  fresh baseline6775ms, killed1/survived0, complete execution, peak owned scratch
  54336654bytes and clean cleanup. The later mandatory-tool-set addition retains
  that predicate; its missing-timeout/prlimit fixtures and happy path are checked
  separately.
- Final native ARM64 integration review: the completed image passed strict
  preflight for all 11 real executables and its pinned configuration/receipt.
  Creusot generated a nonempty named Coma file and proved `vc_add_one` separately
  for each required target, with `--no-cache` and `-j 2`. A first warm repeat
  reused Cargo's generated output despite rerunning provers; a second repeat
  touched only the disposable sample source and observed `Checking smoke_crate`
  and successful proof on every target. This confirms the local smoke path,
  while Task 4 still needs an owned fresh target or package clean for product
  source, native x86_64 setup/proof timing and fail-closed proof gates.
- Final test/evidence review: all five implementation and five testing passes
  above remain applicable after late installer repairs. The native attempt and
  warm-repair measurements, exact final image ID/size, 13 untouched unrelated
  containers, Docker VM headroom, Python 3.12.3 and scripts/logs are in the
  Task 2 report. Focused checker32, Ruff format/lint, ty, shell/embedded Python
  syntax and normal commit hooks are rerun before the Task 2 commit; no full
  Python suite is repeated without a new suite-wide risk.

### Task 2 independent-review correction: selected project configuration

The reviewer found that preflight hashed only the installed Why3find template,
while cargo-creusot starts Why3find from the project root and selects that
project's `why3find.json`. This was a real gap: changing the selected project's
`provers`, `time` or `tactics` left the old checker green. The new proof-facing
`check --manifest ... --project-root PROJECT` requires a project-local file
whose exact hash matches the manifest's pinned `project_config`; missing,
changed, or unspecified project roots fail. `check-installation` retains the
image self-check before any project exists and is documented as insufficient
before proof. The smoke invokes the proof-facing check against its actual root;
the Dockerfile and configuration stage use installation-only.

Behavioral RED was observed for all three changed project settings and a
missing project file: the old checker returned exit 0. A separate RED showed
the old `check` accepted no project root, and the old CLI rejected the new
installation-only command. The focused suite then passed all 38 cases.
Actual ARM64 overlay checks passed matching project configuration and rejected
changed `time` and a missing file. The revised ordinary-crate smoke generated
and proved its named goal separately for all three targets and again after
source-triggered warm regeneration. Source/tool pins, resource limits,
production math and required targets are unchanged. This overlay validation
reused the already verified tool image; native x86_64 full-job acceptance
remains Task 4.

## Task 2 independent review completion

The independent scoped review found that the original environment preflight
checked the installed Why3find template but not the selected project file.
Fix `09226fa8` requires the actual project root for proof preflight and rejects
missing or changed configuration. The scoped re-review found the issue
addressed and no new Critical/Important problems. The controller inspected
the actual project rejection log and all six first/fresh-warm sample proof
results. Task 2 is complete; production arithmetic proofs and native Linux
x86_64 cold/warm acceptance remain Tasks 3 and 4, respectively.

## Task 3 production contracts: self-review passes

These passes concern the actual budget arithmetic package, not the environment
smoke. Evidence and repair history are in the worktree-local Task 3 report.

| Pass | Requirements review | Design review | Implementation review | Proof/test review |
| --- | --- | --- | --- | --- |
| 1 | Mapped all eight approved mathematical clauses to production functions. | Models are integer nanoseconds; runtime types remain unchanged. | Constructor rejects exactly zero and preserves every positive Duration. | Existing independent arithmetic properties cover zero, maxima, fractions and arbitrary inputs. |
| 2 | Kept all usize/Duration inputs; no configuration-only bounds. | NonZeroDuration alone owns positivity; public projection has no provenance invariant. | Accessor and Fixed selection use the invariant; Auto retains both overflow paths. | Initial no-cache RED exposed unsupported try_from and missing equality refinement contracts; logs preserve failures. |
| 3 | Required ceiling, strict comparison and exact reconstruction separately. | Private reconstruction isolates its bounded conversion requirement. | Its caller proves Dmax bound; quotient/remainder casts preserve integer value. | Reconstruction has an exact-value postcondition and overflow/panic obligations; existing fraction/max tests exercise runtime. |
| 4 | Required same production bodies for ordinary and proof builds. | Common explicit equality implementations avoid two derive expansion paths. | Structural models for enum/projection include every field; equality contracts prevent omissions. | Ordinary all-features builds and full workspace tests accompany formal equality/refinement checks. |
| 5 | Three independent target generations, no-cache solving, four compiling mutations and fixed solver caps remain mandatory. | Manifest records mathematical contract expressions and generated names, not only counts. | Owned trust denial and conditional dependency boundary audited; upstream contracts explicitly trusted. | Fresh package compilation precedes proof/replay; mutation success means compilation plus the relevant unproved VC. |

Design diagnostic score: 10/10 (8 of 8): the arithmetic module has one purpose;
its caller interface is smaller than the implementation; changes stay behind
the existing API; comments explain positivity and bounded reconstruction;
review explicitly covers design; the module owns saturation/ceiling knowledge;
module boundaries are documented; proof-driven refactoring is included rather
than hidden in adapters. The explicit equality bodies add maintenance compared
with derives; structural deep-model contracts are the corresponding guard.
Final target, sensitivity and regression results are appended after execution.

Task 3 final evidence: each of the three required Rust targets freshly generated
25 Coma files/59 named goals and passed both no-cache prove and fresh-generation
no-cache replay. Source hashes stayed unchanged and matched the host worktree.
Cold-crate plus fresh-warm six-run sequence121.98s, peak process/child RSS1490344
KiB (generation) and272072KiB (proof); each proof8.67–10.62s. This ARM64 development
host does not establish the native Linux x86_64 cold-environment CI deadline.
All four required production arithmetic mutations compiled and produced their
relevant unproved goals; a fifth omitted-field equality mutation did likewise.
They were restored before final proofs. Stable all-features full workspace:
2916 passed/0 failed/33 ignored across171 summaries; no broad rerun or unreported
failure. Clippy's initial unnecessary lint expectation was removed, then final
workspace/all-targets/all-features Clippy passed21.84s. Normal/build dependency
tree excludes Creusot; source files of all three pinned registry proof crates
match the installed v0.14.0 tree. Native x86_64 CI acceptance and the dependency
license integration audit remain the controller's later tasks.

## Task 3 independent review completion

Commit `68ea9956` passed independent specification and quality review with no
Critical, Important or Minor findings. The reviewer checked all eight clauses,
the shared production bodies and complete typed domains, exact reconstruction,
positive-duration and handmade-projection semantics, six fresh proof rounds,
source hashes, the 59-goal inventory and 75 saved target strategies. All five
compiling sensitivity cases were detected before final source restoration and
successful proofs. The controller also inspected the full stable test summary
(2,916 passed, no failures) and actual proof logs. Tasks 4 and 5 still own the
mandatory CI/native x86_64 setup measurements and final dependency/distribution
acceptance.

## Task 4 mandatory proof gate: self-review passes

| Pass | Requirements review | Design review | Implementation review | Testing review |
| --- | --- | --- | --- | --- |
| 1 | All eight clauses and ten named production functions remain mandatory. | The approved manifest digest binds functions, contracts, logical definitions, models, target list and goal inventory. | Source attributes are compared as complete attached sets, ignoring whitespace only; extra requires and weakened ensures fail. | A joint source-plus-manifest weakening initially passed, then failed after the approved-scope guard; separate source mutation cases fail. |
| 2 | Each of three targets must be freshly generated, complete and separately proved. | Only the owned generated package tree is cleared; package clean forces compilation while dependency caches survive. | Fresh timestamp, nonempty Coma, required markers and named goal inventory are checked before proof. | Missing, empty, old and marker-damaged artifacts fail; a fixture crossing all three subprocess boundaries passes only after three results. |
| 3 | Tool pins and 30s/2GiB/max-two solver limits may not drift. | Selected-project preflight runs before generation and again before proof; pinned solver configuration owns per-process limits. | Proof uses `--no-cache --replay -j 2` on each target; the runner has an 1800s overall deadline and preserves step logs. | Missing/failed tool and deadline cases fail; a solver timeout followed by a successful alternative is accepted only with complete final goal success. |
| 4 | Deadline and interruption must stop owned solver descendants without affecting other processes. | Every command starts a new session; Linux cleanup kills the dispatcher's group first, then scans only its session for separated child groups. | The runner reaps the leader after scoped SIGKILL; normal successful runs do not invoke cleanup. | A real Linux child in a separate process group survived the old cleanup (state S), then terminated (state Z); ordering test failed child-first and passed leader-first. |
| 5 | CI must select proof for math/spec/tool/dependency changes and reject failed, cancelled or unintended skips. | `creusot` is a distinct selector output and mandatory aggregate need; cold tool-image cache covers every effective recipe input without solver-result cache. | Job has a 30-minute cap, native x86_64 check, diagnostics and per-attempt proof report. | Selector/aggregate, workflow structure, cache-key coverage and pinned workflow linters pass; native cold/warm dispatch remains the final Task 4 gate. |

Task 4 local pinned Linux ARM64 container: cold crate build81.379s and warm
fresh replay35.438s. Every target generated25 Coma files and proved59 named
goals with25 saved strategies and no success-result cache. Cold generation peak
process/child RSS1,491,140KiB; proof peak271,264KiB. This is a local crate
cache measurement after the tools were installed, not the native x86_64 full
setup-plus-proof acceptance. Full Python suite:1,179 passed,12 skipped. The
first sandboxed run's seven unrelated environment failures were repaired by
using a task-owned uv cache and granting the existing process-monitor tests
their required OS access; no test was deleted or marked xfail. The native CI
cold image-cache miss and warm hit results follow.

Task 4 native CI at proof-source commit `41c888f5`: the warm attempt in
[run 39191762043](https://github.com/HayaoSuzuki/hoimin/actions/runs/39191762043)
hit the pinned tool-image cache, then regenerated and proved each target's
25 Coma files/59 goals without solver-result cache; full job240s, proof runner
153.348s. The complete attempt-1 workflow succeeded, including the mandatory
aggregate. After deleting only the exact feature-branch image cache, the same
SHA's Creusot job was rerun as attempt2. Its log shows a genuine cache miss,
native image rebuild/save, and all three fresh no-cache proofs in942s including
setup, within the fixed1800s job cap; runner152.438s. Cold peak process/child
RSS1,507,040KiB generation and273,260KiB proof, tool-build step RSS1,087,072
KiB; disk81,462,316KiB and available memory14,483,712KiB afterward. The
earlier [run 39190445969](https://github.com/HayaoSuzuki/hoimin/actions/runs/39190445969)
had a cache miss and completed image build but failed before proof due Python3.12
syntax; it is failure evidence, not cold acceptance. Repairs also tracked the
real test fixture absent in that run's checkout. Final `4d4a3ec6` changes only
test portability and the new math Cargo metadata file's checkout EOL policy;
runner, math, pins, recipe and workflow bytes remain those accepted at
`41c888f5`. A manual Windows test run exposed POSIX fake-tool fixture failures,
default cp1252 proof-log decoding, and one CRLF sdist-byte mismatch; 13 failures
were repaired in `4d4a3ec6`, and the corrected non-Linux run later passed
all nine jobs. Portable validators and three-target orchestration remain
active on Windows. The subsequent scoped review found the two Important gaps
repaired in the fix round below.

## Task 4 scoped-review fix round 1

Independent review at `4137a294` found two Important gaps: alternate valid
Creusot attributes could narrow a public input domain without entering the
source inventory, and default SIGTERM could leave the command's Linux session
alive. The source guard now compares every attached outer attribute on each
reviewed production function with the exact approved manifest forms; only
`#[must_use]` is exempt. It rejects alternate cfg predicates, qualified names,
compound/nested contracts, same-line attributes and unsupported syntax.
Comments between attributes are traversed. Production arithmetic, contract
manifest, tool pins, targets and resource caps are unchanged. The first repair
caught SIGTERM during each subprocess command and killed the dispatcher
before separate solver groups. Its temporary Linux `Popen` signal mask was
subsequently found to be inherited by children, so round 2 below replaces
that part while preserving owned cleanup and handler restoration. The unresolved Coma source-span warning is deferred
to Task 5/final review.

| Fix pass | Implementation review | Testing review |
| --- | --- | --- |
| 1 | Exact attached-attribute multiset replaces a literal-prefix filter. | Three alternate/nested cases first passed incorrectly, then failed; canonical contracts remain accepted. |
| 2 | Line/block comments and same-line syntax cannot hide a contract attribute. | Two comment-separated and one inline case were behavioral RED before their scanner fixes; all are GREEN now. |
| 3 | Scoped SIGTERM handler restores the previous handler on failure. | Failure-path handler restoration is asserted; host focused suite passed 37/2 platform skips. |
| 4 | Linux launch masking and owned-session cleanup preserve dispatcher-first order. | Actual separate-PGID child reached state Z after SIGTERM; wrapper returned failure code 17. |
| 5 | No math, manifest, recipe, caps or target changes. | Final Python 3.12 runner proved all three targets afresh, each 25 Coma/59 goals/25 strategies in 34.459s; a focused guard mutation was killed. |

The full Python suite passed `1,187` tests with `13` skips under the established
process-permission and task-owned uv cache setup. Final focused tests, Ruff,
type checking and formatting passed after the last small test additions. The
local runner and container source hashes matched
`986747a2d2a5d227029700982323ceba111036935ec570f04d2e5ac85ff11430`.
Full local logs and the scoped RED/GREEN details are in the worktree-local
`task-4-report.md` and `task-4-evidence/review-fix-py312/`. Earlier native
warm/cold acceptance at `41c888f5` remains historical evidence for those
runner bytes; native execution of this fixed runner and scoped re-review are
pending. The corrected non-Linux run at `4d4a3ec6` completed all nine jobs
successfully: https://github.com/HayaoSuzuki/hoimin/actions/runs/39193858954 .

## Task 4 scoped-review fix round 2

The first fix's scoped re-review accepted the source-contract inventory and
parent SIGTERM cleanup, then found that masking SIGTERM before Linux `Popen`
caused every launched proof tool to inherit the blocked signal. The corrected
runner leaves the signal mask unchanged. Its temporary handler records a stop
while `Popen` is assigning the owned PID, then enters owned-session cleanup
once that PID exists; later SIGTERM raises directly. Cleanup runs only before
`Popen` has reaped its leader, avoiding a late signal against a reused PID.
It still stops the dispatcher group before separately grouped children and
restores the previous handler on exit. Production math, contracts, tool pins,
three targets and all resource caps remain unchanged.

Five focused review passes: (1) real Linux child signal delivery failed before
repair (`wrapper=17`, handler marker absent) and passed after (`wrapper=0`,
marker present); (2) real parent SIGTERM still returned failure and stopped its
separately grouped child (state Z); (3) an injected signal during `Popen`
deferred cleanup until PID assignment; (4) an injected signal after leader reap
did not call session cleanup; (5) Python 3.12.3 ran fresh no-cache proofs for
all three targets, each 25 Coma files/59 goals/25 strategies, in 35.851s.
Focused host tests passed 39 with three Linux-only skips; Ruff, formatting,
type checking and diff checks passed. Full Python/Rust suites were not rerun
because only this runner lifecycle changed. Final runner SHA-256:
`74540a367029ea3c0cd9e76eed8042d342c85c577c7173288c3616cb870116e0`.
The complete local report and logs are in worktree-local
`task-4-report.md` and `task-4-evidence/fix2-py312/`. Final native current-lock
CI and scoped re-review remain pending with the controller.

## Task 5 checkpoint: diagnostic spans and Windows oracle premise

At integration base `878be0d5496be231f0f15b6da1a8fe8f03659e6c`, the retained
Windows failure (run `39192791500`, job `117869024599`) showed zero executed or
durable results in `stop_preserves_accepted`. That platform alone used an
unguarded one-second total timeout. Both platforms now use the existing
12-second timeout helper, which requires one persisted killed result within
eight seconds before comparing the unchanged Lean expectation. Failure to reach
that premise is infrastructure error. Existing bounded Unix/Windows process-tree
cleanup remains in use. The obsolete boolean timeout option was removed.

The pinned Creusot source and actual `cargo creusot --help` confirm absolute
spans are supported. Fresh three-target proof with `--span-mode absolute` and
the current merged Cargo.lock passed: each target generated 25 files and proved
59 goals with no-cache replay. All three proof logs contain zero source-span
warnings, replacing the prior 393 warnings per target. Historical fixture logs
remain unchanged. Absolute paths refer to the job's live source tree; archived
Coma diagnostics need that original path or regeneration.

Focused host validation: 39 runner tests passed, three Linux-only tests skipped;
all six result-lifecycle tests passed, including real durable result polling and
bounded descendant cleanup. Actual final Windows execution belongs to the
controller's checkpoint CI run. Full Task 5 validation and self-review follow in
the integration report; this checkpoint makes no broader completion claim.


## Task 5 final integration review record

The [integration report](../reports/2026-10-19-issue-999-creusot-adoption.md)
records five implementation self-reviews and five validation self-reviews,
actual root/fuzz license audits, extracted-sdist wheel execution, generated SBOM,
OKF checks and source-specific evidence. Current-source native full CI and
warm/cold proof passed at `5033e1bb`, run `39197995202`, attempts 1 and 2.
The lifecycle adapter follow-up at `312be2c4` passed all nine native non-Linux
jobs in `39202749924`. Final whole-branch review remains the controller's next
gate. All seven controller Rulings and prior task/review
reports survive in the [durable archive](../reports/issue-999/README.md).
No final native platform result is inferred from the local package checks.


## Task 5 scoped review and native Windows follow-up

The [scoped review](../reports/issue-999/records/task-5-review.md) approved the
local Task 5 change, with two Minor provenance/inventory corrections. The
[follow-up report](../reports/issue-999/records/task-5-fix1-report.md) records
those corrections, the later actual Windows failure and the scheduling-only
repair. Its five self-reviews preserve the six-second assertions and distinguish
local workflow validation from pending native Windows execution.

The [scoped fix review](../reports/issue-999/records/task-5-fix1-review.md)
confirmed both Minor corrections and found no new breakage in `46ef6b9e..70ea57b2`.
Its native Windows finding remained open. Follow-up `39199825814` failed the
same two readiness cases despite two harness threads. Diagnostic run `39201076428`
then passed both cases but failed the lifecycle adapter's exit guard. The
[round 2 report](../reports/issue-999/records/task-5-fix2-report.md) preserves
those results and the separate contract-derived exit-wait repair in `312be2c4`.
The [round 2 scoped review](../reports/issue-999/records/task-5-fix2-review.md)
found the composed guard sound and no new breakage, with native acceptance still
pending at review time. Run `39202749924` subsequently passed all nine jobs at
`312be2c4`, closing that external gate. The review retains its original status and
its limitation that RED was archived as an exact command/result summary rather
than a raw terminal transcript. The original intermittent readiness cause remains
unproven; final whole-branch review is outstanding.
