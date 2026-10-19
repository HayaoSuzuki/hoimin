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
