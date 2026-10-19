# Task 3 report: production arithmetic proofs

Status: DONE_WITH_CONCERNS (later native x86_64 CI and license integration gates). Base `17135904`; branch `issue-999-creusot-budget-proofs`.
Worktree `.worktrees/issue-999-creusot-budget-proofs`. No nested agents used.

## Implementation and scope

The actual `hoimin-budget-math` package now carries conditional Creusot contracts
for every approved mathematical clause. Positive Duration uses an integer View,
integer DeepModel and positive invariant; the public projection has no extra
invariant. Both proof and stable Rust compile the same production types and
bodies. All public typed input domains are preserved. The logical helpers are
mathematical specifications only; they do not replace the runtime algorithms.

The only arithmetic implementation refactor is private exact reconstruction:
`u64::try_from` lacked a pinned upstream contract and generated an impossible
precondition, so the bounded input is converted by quotient/remainder casts with
an exact-value contract. The caller proves the Dmax precondition. Saturating
product, ceiling, auto timeout and strict shortfall retain their original bodies.

Three ordinary PartialEq derives lacked the trait postcondition Creusot requires.
The parent ruled on 2026-10-19 to use common explicit equality bodies with
conditional contracts, instead of conditional derives with two expansion paths.
NonZeroDuration compares its Duration; MutantTimeout compares variants and fixed
values; TopBudgetProjection compares every public field. Structural derived
DeepModels for enum/projection ensure an omitted equality field is detectable.
All runtime serde implementations and serialized forms remain unchanged.

`creusot-std = "=0.14.0"` is under `target.'cfg(creusot)'.dependencies`; actual
Creusot activates it. Stable all-features normal/build tree contains only serde
and its macros, not proof libraries. Root workspace check-cfg acknowledges the
proof cfg. Root/fuzz locks include proof graph checksums; unrelated Windows
resolution changes from the container were restored to their prior locked graph.
Every Rust source under the three registry packages (creusot-std,
creusot-std-proc, pearlite-syn) matches the installed pinned v0.14.0 source tree.
The crate denies owned `creusot::trusted_code`; no owned trusted, external-spec,
assume or proof-only runtime body has been added. Upstream standard-library
contracts, compiler, solvers and Why3 remain the documented trust boundary.

## Repair history and RED/GREEN

Evidence directory: `.superpowers/sdd/2026-10-19-issue-999-creusot-adoption/task-3-evidence/`.

1. `initial-generation.log`: a Cargo `-p` flag after `--` did not restrict
   Creusot's selected workspace packages; unrelated C dependencies attempted to
   cross-compile and failed for absent x86_64-linux-gnu-gcc (18.68 s). Pinned CLI
   source/help showed package selection belongs before `--`. No compiler was
   installed and proof scope did not expand.
2. `generation-2.log` (15.47 s) through `generation-5.log`: annotation syntax and
   import repairs. Pearlite view body needed the macro form; glob imports caused
   derive ambiguity; ordinary equality required DeepModel; logical visibility
   must match private tuple fields; derive macro is in the prelude, and private
   Duration needs a manual integer DeepModel. These are compile diagnostics,
   explicitly not sensitivity evidence.
3. `generation-6.log`: generation succeeded (0.26 s warm, peak186036 KiB), with
   the actual unsupported try_from warning. Initial Coma saved in `initial-verif`.
4. `initial-proof.log`: observed no-cache RED, exit1, 48.54 s, peak271196 KiB.
   All required arithmetic contracts except saturation already proved. Failures
   were `vc_saturating_duration_mul` (impossible external-conversion precondition),
   three equality `refines` goals and `vc_eq_MutantTimeout`.
5. `repair-proof.log`: first common-body repair missed a DeepModel trait import;
   compile failed, no proof conclusion. `repair-proof-2.log`: generation0.25 s,
   proof9.73 s, peak proof271200 KiB, exit0, all25 generated Coma files proved.
   Actual successful artifacts preserved in `green-initial-verif`.

Every actual proof invocation is preceded by the strict selected-project checker
against `/workspace` and `/workspace/why3find.json`. It is never run against the
template directory in place of the project root. The root JSON matches the
pinned formal JSON byte for byte. Solver invocations retain hard30-second and
AS2GiB prefixes, Why3 memory2048 and `-j 2`; no cap or target was relaxed.

## Verification commands

Stable commands use `CARGO_TARGET_DIR=/tmp/hoimin-issue999-target`,
`CARGO_BUILD_JOBS=2` and the repository Rust1.99.0 toolchain; full Rust tests use
`PATH=/Users/hayao/RustroverProjects/hoimin/.venv/bin:$PATH` for runtime fixtures.

- `cargo test -p hoimin-budget-math --all-features --locked`: 29 arithmetic and
  6 serialization passed (`stable-arithmetic.log`).
- `cargo check --manifest-path fuzz/Cargo.toml`: exit0,17.17 s (`fuzz-lock.log`).
- `cargo tree -p hoimin-budget-math --all-features --edges normal,build --locked`:
  proof dependency absent (`stable-dependencies.log`).
- `cargo test --workspace --all-features --locked`: exit0,171 suite/doc-test
  summaries,2916 passed,0 failed,33 ignored; compilation2m33s
  (`stable-workspace.log`, summarized in `stable-workspace.json`). No environment
  failure and no broad rerun. This includes arithmetic, Lean corpus, config and
  actual runtime/CLI regression checks with the contracts feature.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`:
  initial run rejected an unfulfilled remainder-cast lint expectation; Clippy
  already proves that remainder range. Removed only that annotation; final run
  passed in21.84s (`clippy-fixed.log`). No arithmetic body changed after the
  full suite. `cargo fmt --all -- --check` and `git diff --check` passed.

## CI handoff

`formal/creusot/obligations.json` maps the eight clauses to function-attached
source contracts, complete precondition sets, logical definitions, structural
model requirements, actual Coma paths, generated ensures markers and goal names.
This is stronger than a nonempty-directory or goal-count gate. Task4 must reject
deleted/weakened contracts, extra preconditions and an added public projection
invariant, then prove every generated file including derived-method obligations.
It owns the mandatory runner and final native Linux x86_64 setup-plus-proof
30-minute gate. Task3 local Linux host is native ARM64; target names describe the
verified Rust targets, not native x86_64 host evidence.

## Resources and isolation

Used only task-owned container `hoimin-issue999-task3`, fixed image
`hoimin-issue999-creusot-arm64:fix1-local`, ID
`sha256:f1eec1050faa3c0af5e540eabc114c165c308abfe4fcdc585e3437b1a23e8603`.
No proof tool rebuild or image publication. Initial host free95GiB; container VM
24GiB free. Shared host stable target grew to17GiB while relinking full tests;
host free remained94GiB at intermediate measurement. One heavy build at a time;
the stable build completed before target proof compilation. Thirteen unrelated
containers and all other worktrees are preserved. GNU time reports peak process/
child RSS, not aggregate container RSS. Final per-target/cold-warm measurements appear below.

## Self-reviews

Five requirements, five design, five implementation and five proof/test passes
are recorded in `docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md`.
The eight-row design diagnostic is10/10; explicit equality adds maintenance, with
structural-model contracts as the guard. Final proof, sensitivity and regression
evidence follows below. Licensing remains a final integration
audit concern: cfg exclusion does not itself resolve upstream LGPL declarations.

## Final three-target production proof evidence

After all sensitivity runs restored the exact source, cleared only the task-owned
Linux `/tmp/hoimin-issue999-target` directory and ran `three-targets.sh` from the
evidence directory in the pinned container. No host build cache was deleted.
The exact script is preserved and contains all generation/prove/preflight,
package-clean and replay commands. Outer command:

```sh
docker exec hoimin-issue999-task3 bash -c \
  'rm -rf /tmp/hoimin-issue999-target; /usr/bin/time -v /bin/bash /tmp/task3-three-targets.sh'
```

This command exited0. Complete six-round wall time121.98s, peak process/child
RSS1490344KiB. This is a cold crate target directory with already-installed tools
and cached registry downloads, not a cold environment installation. No native
Linux x86_64 host deadline claim is made. Final evidence is under
`task-3-evidence/final/{TARGET}/{first,warm}/`: separate preflight, generation,
proof, source-check logs and all generated Coma plus proof strategies. The first
proof creates a strategy without result-cache reads. The warm round cleans only
the owned package, deletes its prior generated output, actually regenerates
(`Checking hoimin-budget-math` is present), copies that target's first strategy,
and executes `--replay --no-cache -j 2`. Every round checks the source SHA256 set
has remained identical. `final/final-source.sha256` also matches current host
Cargo manifests/lock, all three production source files and workspace JSON.

| Target | First generation seconds / peak KiB | First proof seconds / peak KiB | Fresh warm generation seconds / peak KiB | Fresh warm replay seconds / peak KiB |
| --- | ---: | ---: | ---: | ---: |
| x86_64-unknown-linux-gnu |20.65 /1488572|10.62 /272072|0.31 /185476|8.67 /271260|
| aarch64-apple-darwin |15.79 /1490344|8.99 /271260|0.21 /188076|8.68 /271192|
| x86_64-pc-windows-msvc |13.94 /1486088|9.00 /271264|0.23 /182888|8.73 /271264|

Every run:25 Coma files,59 named goals, final `Proved (25 files)`, no unproved
goal and exit0. `final/summary.json` contains the measured values. Named semantic
goals are `vc_new`/`vc_get` under `duration/impl_NonZeroDuration`,
`duration/auto_mutant_timeout:vc_auto_mutant_timeout`,
`projection/project_top_budget:vc_project_top_budget`,
`projection/saturating_duration_mul:vc_saturating_duration_mul`,
`projection/duration_from_nanos:vc_duration_from_nanos`, and
`projection/impl_TopBudgetProjection/is_shortfall:vc_is_shortfall`.
The complete59-goal path-qualified inventory is committed in `obligations.json`,
including equality, refinement, clone/debug and imported-operation obligations.
All required generated markers and source-contract/model expressions were
checked against actual output/current source. Saved strategies are under
`formal/creusot/proofs/TARGET/hoimin_budget_math_rlib/.../proof.json` (75 small JSON
files total); there is no saved successful-result cache or generated Coma in the
commit. Each target's exact strategy has been replayed with cache reads disabled.

VM filesystem changed from73327500KiB used/24038496KiB available to73518944KiB
used/23847052KiB available: observed191444KiB delta, subject to unrelated activity.
Owned Linux target187644KiB and evidence3716KiB. Final host available100645624KiB.
Final Docker listing contains14 running containers:13 unrelated plus this task's
idle container. It is retained for independent review; no active proof/build
remains. Final memory snapshot during last replay was102.1MiB; this snapshot is
not a peak. All process/child peak figures above are GNU time values, not summed
container/environment RSS. The fixed solver AS2GiB/30s/max2 constraints remained
active in all runs and were checked before each invocation.

## Final proof sensitivity evidence

The exact disposable harness is `task-3-evidence/sensitivity.py`; it changes one
production expression at a time in the container, compiles the complete actual
crate on the x86_64 Linux target, proves the relevant freshly generated file with
`--no-cache -j 2`, requires nonzero proof exit and the named unproved goal, then
restores the original file. A `finally` block restores both source files even
on harness error. Host production source was never mutated. Per-case directories
contain exact before/after replacements, compiler/preflight logs, source SHA256,
the generated relevant Coma and proof log. All five cases have generation exit0
and proof exit1; no compile failure is counted as sensitivity evidence.

| Case | Production change | Required unproved goal | Proof seconds |
| --- | --- | --- | ---: |
| ceil | Drop remainder increment from waves |vc_project_top_budget|26.709|
| saturation | Return Duration::ZERO for product above Dmax |vc_saturating_duration_mul|11.303|
| auto | checked_mul(2) to checked_mul(3) |vc_auto_mutant_timeout|15.419|
| equality | projected_capacity > remaining to >= |vc_is_shortfall|14.784|
| structural_equality (additional) | Omit projected_capacity from equality body |vc_eq_TopBudgetProjection|20.183|

Results are summarized in `sensitivity/results.json`; the final passing
three-target proofs above were run only after restoration. The fifth case checks
the maintenance guard added for common explicit equality bodies.

## Changed files and remaining integration concerns

Changed: root Cargo.toml/Cargo.lock and fuzz/Cargo.lock; budget-math Cargo.toml
and its three source files; root why3find.json; formal/creusot README,
obligations.json and75 target-specific proof strategies; durable review notes.
The task-local report/evidence is ignored by Git and retained in the worktree.
No runtime arithmetic tests were mirrored/reimplemented: existing independent
properties, finite boundary tests and Lean/runtime correspondence remain the
regression oracle; proof contracts supply exhaustive mathematical specifications.

Task3 is complete after the normal-hook commit recorded below. Task4 must still
implement/enforce mandatory runner scope/failure/freshness gates and obtain the
native Linux x86_64 cold/warm setup-plus-production-proofs <=30min acceptance.
Final integration must audit proof-only LGPL dependency declarations and normal
3OS quality gates. These are explicit later-task concerns, not local proof
failures. No caps, targets, public domains or semantics were relaxed.


## Commit and final verification gate

Commit `68ea9956d63c13033a4f4a4153099e3e08cebd89`:
`Prove production budget arithmetic on three Creusot targets`.
Normal commit hooks ran without bypass: workspace and vendored-parser formatting,
workspace Clippy and vendored-parser Clippy all passed. Exact output is
`task-3-evidence/commit.log`. Final `git status --short` was empty; no push.
Final pre-commit artifact audit verified identical pinned root configuration,
all source contract/model tokens,25 strategies per target,59 named-goal inventory,
and current production source/manifest/lock hashes against the six passing
proof runs. No production changes followed that audit. The ignored report and
evidence remain available in this worktree, and the idle task container remains
available for independent review.
