# Production budget proofs and pinned environment

The production proof is `python3 tools/verify_creusot.py --output /proof-report`
inside the pinned image, from the repository root. The runner enforces source
and contract inventories, fresh generation, named nonempty goals, preflight
before generation and proof, and final success for all targets. `CI result`
requires this selected job to succeed. Ordinary stable builds need none of the
proof tools. See the [adoption record](../../docs/superpowers/reports/2026-10-19-issue-999-creusot-adoption.md)
for source revisions, independent reviews and archived execution evidence.

The mandatory requirement is enforced by the workflow's `CI result` aggregate:
a selected proof failure, cancellation, timeout or unexpected skip fails CI.
It does not assert GitHub merge enforcement. The controller's October 19
[settings observation](../../docs/superpowers/reports/issue-999/evidence/final-publication/branch-protection-observation.json)
found deletion/non-fast-forward protection on `main` but no required-status-check
rule; legacy contexts/checks were empty with enforcement off. No setting changed.

For a local Linux x86_64 checkout, build the image below, create an empty report
directory, then use the same invocation as CI:

```sh
mkdir -p /tmp/hoimin-creusot-report
docker run --rm --platform linux/amd64 \
  -v "$PWD:/workspace" -v /tmp/hoimin-creusot-report:/proof-report \
  -e CARGO_TARGET_DIR=/tmp/hoimin-issue999-target -e CARGO_BUILD_JOBS=2 \
  --workdir /workspace hoimin-issue999-creusot:local \
  python3 tools/verify_creusot.py --output /proof-report
```

Use a fresh report directory per run. The runner removes only its generated
crate output and cleans only `hoimin-budget-math` for the selected target. It
keeps the strategy files under `formal/creusot/proofs` as inputs and invokes the
solvers again. Absolute spans resolve diagnostics against that run's source
checkout; regenerate after moving the checkout instead of expecting archived
absolute paths to resolve elsewhere.

## Repairing and updating proofs

Read the failing target's `generation.log` or `proof.log` and `summary.json`.
First reproduce with the pinned environment and current source. Compiler errors,
missing tools, invalid inventories and timeouts are failures, not unproved
mathematical counterexamples. For a final unproved goal, inspect its named
contract and the actual production body; keep all public typed input domains.
After changing contracts or bodies, review `obligations.json` against the eight
requirements and regenerate on all three targets. To repair a stale strategy,
run the manual generation/proof commands below without `--replay`, save the
resulting target's `proof.json` files in its strategy directory, and finish with
the full runner's fresh no-cache replay. Retain compiling negative sensitivity
cases as evidence; a parse failure does not establish contract sensitivity.

Tool updates change the exact source/artifact hashes, version lines, configs,
receipts and relevant manifests together. Rebuild, rerun environment negative
tests and all targets, then measure native cold and warm setup-inclusive CI.
Review both lockfiles, licenses, proc-macro output and distribution inputs.
Do not increase the accepted solver/job limits to hide a regression. Stable
Rust 1.99.0 and the unrelated fuzz/random-order nightly remain separate pins.

## Accepted native cost and proof scope

At source `41c888f5`, native Linux x86_64 run `39191762043` measured 240 seconds
warm (tool-image cache hit) and 942 seconds genuinely cold (cache miss), including
setup. The proof runners took 153.348 and 152.438 seconds. Both regenerated
25 Coma files and proved 59 goals per target. Cold generation peaked at
1,507,040 KiB process-or-child RSS; cold proof at 273,260 KiB. The cold tool-build
step peaked at 1,087,072 KiB RSS. Image size was 3,933,017,305 bytes and the saved
tar used 3,976,204 KiB. After cold completion the runner had 81,462,316 KiB disk
and 14,483,712 KiB memory available. These are observed process/host figures,
not an aggregate whole-environment RSS bound. The earlier failed cold run
`39190445969` is retained in the report as a failed attempt.

At code checkpoint `5033e1bb`, native warm run `39197995202` passed in
247 setup-inclusive seconds (runner 156.09 seconds), with 25 files / 59 goals
per target and no source-span warnings. Its generation/proof peak process-or-child
RSS was 1,514,396 / 273,676 KiB. The same source's genuine cold attempt 2
(explicit tool-image cache miss) passed in 693 setup-inclusive seconds; runner
105.023 seconds, generation/proof peak RSS 1,516,044 / 273,544 KiB, tool-build
peak RSS 1,086,792 KiB. Each target again generated 25 files and proved 59 goals
with 25 strategies, without source warnings; both reports totaled 29,542 bytes.
Cold completion left 81,462,580 KiB disk and 14,552,420 KiB memory available.
Separate-job timings do not establish a causal warm/cold proof-speed difference.
Both native workflow attempts concluded success; attempt 2 reran the proof job.
Task 5's current-lock ARM64 development replay also passed in 34.133 seconds
without source-path warnings. Native Windows/macOS regression results remain separate acceptance evidence:
run `39202749924` at `312be2c4` passed all nine jobs after the lifecycle adapter
repair. Earlier native failures and the unproven intermittent readiness cause
are preserved in the report; arithmetic proofs do not establish OS timing.

The arithmetic contracts cover all public `usize`, positive `NonZeroUsize`,
`Duration` and positive fixed durations. They do not prove BudgetLedger state
transitions, reservation ownership, persistence, process scheduling, wall-clock
timing, OS APIs, serialization or unsupported targets. Lean corpus correspondence,
property tests, native process tests and runtime contracts cover separate parts
of the system. The top-budget projection remains warning-only.

The annotation/equality/reconstruction change (`17135904..68ea9956`) added 119
and removed 7 source lines across the three math modules, a net 112 lines.
The final modules total 238 lines; the explicit obligation inventory is 413
lines and the runner is 640 lines. These counts include whitespace and comments;
they describe maintenance size, not proof strength. Five compiling sensitivity
changes to ceiling, capacity saturation, auto timeout, strict shortfall and
structural equality each failed their corresponding proof goal.

## Environment installation

`toolchain.json` records the Linux x86_64 installation inputs, source revisions,
SHA-256 hashes, exact executable version lines, configuration hashes and required
environment selection. `Dockerfile` and `install.sh` install only into the owned
image under `/opt/creusot`. No proof tools are needed for ordinary product builds.

The trust boundary includes the official Rust/Creusot standard-library contracts,
compiler, Why3/Why3find and Z3/CVC5. Hash receipts detect installation drift; they
are build provenance, not signatures that make an untrusted image trustworthy.
The image must be built from the reviewed recipe and manifest. Registry crates
use the upstream Cargo.lock checksums; opam uses a fixed repository snapshot and
its package source checksums. Ubuntu packages use a dated snapshot whose signed
indexes authenticate package hashes. A dated, SHA-256-verified Mozilla CA bundle
bootstraps TLS before the distro certificate package is available.

Build from the repository root with a small context (do not include Rust targets
or the linked Python environment):

```sh
tar -cf - formal/creusot tools/creusot_environment.py |
  docker build --platform linux/amd64 --no-cache --progress=plain \
    -f formal/creusot/Dockerfile -t hoimin-issue999-creusot:local -
docker run --rm --platform linux/amd64 hoimin-issue999-creusot:local
```

The default container command checks the pinned installation, including all
executables, receipts and installed prover configuration:

```sh
python3 tools/creusot_environment.py check-installation --manifest formal/creusot/toolchain.json
```

Run that command inside the installation environment. It fails on malformed or
incomplete manifests, missing build receipts, changed files, wrong PATH selection,
configuration environment drift, nonzero version commands and mismatching actual
version output. Tool exit codes alone cannot establish the expected environment.
The receipt includes executable hashes, the Rust compiler driver and each target's
standard library. Rebuild after changing the manifest or configuration.

Before **every proof invocation**, run the proof preflight from the same
installation with the Cargo project root that `cargo creusot` will use:

```sh
python3 /recipe/tools/creusot_environment.py check \
  --manifest /recipe/formal/creusot/toolchain.json \
  --project-root /workspace/my-project
```

This also requires a project-local `why3find.json` and compares its exact
content hash with the pinned `project_config` entry in the manifest. Missing
or altered project settings, including prover selection, time and tactics,
fail closed. The installation-only command does not establish a project's
effective Why3find configuration. The production runner performs this check
against the actual project root before using `cargo creusot` there; the root
must match the workspace root selected by cargo-creusot.

The approved targets are `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` and
`x86_64-pc-windows-msvc`; their dated Rust standard-library artifacts are installed
separately. A successful download is not proof of compiler or Creusot target
support. Record actual per-target compiler invocations and proof generation.

Proof invocations must pass `-j 2` explicitly: cargo-creusot otherwise supplies a
CPU-derived parallelism argument to Why3find. Both Why3 configurations include
`running_provers_max = 2` and `memlimit = 2048`; the configuration used by
cargo-creusot's WHY3CONFIG override includes exact Z3/CVC5 versions. Other solvers
are neither installed nor selected. See the pinned upstream
[cargo-creusot wrapper](https://github.com/creusot-rs/creusot/blob/65349025c67e69fcd7479f1f420fa5904f69bc27/cargo-creusot/src/why3find_wrapper.rs).

Why3find's `time` is a calibrated reference time. Its final attempts can be
multiplied and its process monitor allows additional wall time. Therefore the
JSON `time` value does not enforce Q7's actual 30-second solver deadline. The effective solver commands therefore use absolute GNU `timeout --signal=KILL 30s`
and `prlimit --as=2147483648` prefixes. Preflight checks the actual timeout/prlimit
executables as well. RLIMIT_AS limits address space and is stricter than a
2-GiB RSS limit; it is not a whole-environment memory bound. The mandatory runner
must preserve these limits, bound concurrent solvers to two and enforce the
whole setup-plus-three-target job's 1800 seconds.
Three targets run sequentially. This environment recipe does not claim a strict
whole-environment RSS bound. See the pinned
[runner](https://github.com/creusot-rs/why3find/blob/0f054b93c86ac7ba20bca7df5d1563a9a4434a30/src/runner.ml).

Local timing uses an ARM64 Rancher Desktop engine with four CPUs and
8,321,900,544 bytes RAM. `docker run --platform linux/amd64 ... uname -m` returned
`x86_64`; execution is emulated. Existing unrelated containers remain running.
These local measurements cannot establish the native Linux x86_64 CI deadline.
Measure the complete native job, including image pull/setup, against 30 minutes.
The local pre-pull for the architecture probe was not timed independently, so a
later build-only time is not a complete cold setup time. Task 2's report records
each failed attempt, measured resources and remaining acceptance checks.

The local ARM64 development variant uses the same source revisions, proof
configuration and three targets. Only the host compiler/standard-library and
solver distribution artifacts differ. Build it with:

```sh
tar -cf - formal/creusot tools/creusot_environment.py |
  docker build --platform linux/arm64 --no-cache --progress=plain \
    --build-arg CREUSOT_BASE=ubuntu@sha256:18b33ff3829707f31ada654a963b9ee35f2e6f0241b964bd17a22a9b330cb63d \
    --build-arg TOOLCHAIN_MANIFEST=toolchain-arm64.json \
    -f formal/creusot/Dockerfile -t hoimin-issue999-creusot-arm64:local -
docker run --rm --platform linux/arm64 hoimin-issue999-creusot-arm64:local
```

Both image variants install their selected manifest at the canonical
`/recipe/formal/creusot/toolchain.json` path. The Docker build derives an
artifact-only input document before the heavy tool layer, so configuration,
README or preflight changes can reuse the already built tools. Omit
`--no-cache` for an ordinary warm rebuild. Rust/components, OCaml and proof-tool builds have separate cache layers. The
installer removes obsolete archives and extracted Rust component sources after
successful installation; the small verified Why3/Why3find archives remain local
opam source inputs with explicit checksums. Checksum requirements remain enabled
for all package downloads, including those local archives.

The pinned Linux limit probe exercised the exact 30-second timeout: exit 137,
with the sleeping descendant gone (30.581 seconds including container startup).
A child under the actual AS limit reported 2,097,152 KiB; a tiny 1-MiB AS fixture
failed to map libc and exited 127. No large buffer or host-wide memory pressure
was used. These checks establish the process-limit primitives; the final runner
also checks concurrency, fresh obligations and final proof status.

Run the ordinary-crate smoke in the completed local ARM64 image from the
repository root:

```sh
docker run --rm -i --platform linux/arm64 \
  --entrypoint /bin/bash hoimin-issue999-creusot-arm64:local -s \
  < formal/creusot/smoke.sh
```

The script creates a temporary crate inside the container, points its
`creusot-std` dependency at the exact installed Creusot source, checks the
installation and selected project configuration, and generates and proves the
named `vc_add_one` obligation on all
three targets. It repeats with warm build caches, touching only the temporary
sample source so Cargo actually regenerates each Coma file; `--no-cache`
forces a solver run for every proof. The generated crate and caches disappear
with `--rm`. This is a tool smoke, not the product's arithmetic verification.
The production runner adds source-freshness and proof-status gates.

On this Rancher Desktop host, a bind mount from `/tmp` into the Docker VM
failed because the VM did not see that source path. Streaming the script on
standard input worked. Task 3 can place task-owned source in a container via
standard input or `docker cp`; do not assume host `/tmp` is mountable. The
observed image interpreter is Python 3.12.3.

## Production arithmetic contracts

The verified package is `hoimin-budget-math`, the same package used by the
application. Proof imports, logical models and contracts use `cfg(creusot)`;
production types and function bodies are common to stable Rust and Creusot.
`creusot-std = "=0.14.0"` is a target-cfg dependency, so normal builds and
all-features builds do not compile it. The root and fuzz lockfiles still record
its resolved source checksums. Its upstream license declarations remain part of
the dependency audit even though it is excluded from ordinary target builds.
The verified crate denies `creusot::trusted_code`; it contains no owned external
specifications or assumptions. Upstream standard-library contracts remain in
the trust boundary described above.

`Duration` is modeled as integer nanoseconds, with
`Dmax = u64::MAX * 1_000_000_000 + 999_999_999`. The contracts cover:

| Requirement | Production contract |
| --- | --- |
| Mathematical ceiling, including zero and integer maxima | `project_top_budget`: `waves = (selected + jobs - 1) / jobs` over mathematical integers |
| Both automatic-timeout saturation steps and floor | `auto_mutant_timeout`: `max(5s, min(Dmax, 2 * baseline + 1s))` |
| Fixed/Auto selection | `project_top_budget`: case split over the actual `MutantTimeout` |
| Duration and intermediate-u128 product saturation | `saturating_duration_mul`: `min(Dmax, duration * factor)` |
| Exact seconds/nanoseconds reconstruction | `duration_from_nanos`: result equals its bounded integer input |
| Strict shortfall, including equality | `is_shortfall`: compares the two fields for any public struct value |
| Input preservation | Five separate postconditions on `project_top_budget` |
| Positive duration and accessor meaning | `NonZeroDuration` invariant, constructor and accessor contracts |

The private reconstruction helper requires a bounded total; its caller proves
that requirement. The whole public input domain remains unrestricted apart
from the actual positive input types. In particular, there is no jobs <= 256 or
100-year bound in these contracts and no constructor-provenance invariant on
`TopBudgetProjection`. Standard equality derives were replaced with common
explicit bodies and structural deep-model equality contracts because the
standard derive did not provide the trait postcondition required by Creusot.
The derived models for the enum and public projection include every field, so
omitting a field from their explicit equality prevents the proof from passing.

The workspace-root `why3find.json` is byte-identical to the pinned configuration
in this directory. Cargo-creusot selects the workspace root, so checking only
this template directory would not validate the effective project configuration.
Use `-p hoimin-budget-math` **before** the `--` separator; Cargo flags after that
separator do not select Creusot's workspace package set.

From the actual workspace in the pinned Linux environment, run each target
sequentially:

```sh
export CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-issue999-target
python3 /recipe/tools/creusot_environment.py check \
  --manifest /recipe/formal/creusot/toolchain.json --project-root "$PWD"
cargo clean -p hoimin-budget-math --target "$target"
# Remove only this job's generated output, retaining saved strategies elsewhere.
rm -rf verif/hoimin_budget_math_rlib
cargo creusot -p hoimin-budget-math --only=coma --span-mode absolute -j 2 \
  -- --lib --locked --target "$target"
python3 /recipe/tools/creusot_environment.py check \
  --manifest /recipe/formal/creusot/toolchain.json --project-root "$PWD"
cargo creusot -p hoimin-budget-math --only=prove --no-cache -j 2 \
  --why3find-arg=--goals
```

Cleaning only the owned package forces fresh generation without changing product
source or rebuilding its dependencies. Fresh Coma files, the named contracts in
`obligations.json`, and every final proof status must be checked by the mandatory
runner; a command exit alone or a nonempty directory is insufficient. The
manifest binds source contract expressions and logical definitions as well as
actual generated goals, so deleting an `ensures` cannot leave an apparently
successful overflow-only proof. Changes to that manifest require semantic
review against the eight requirements above.

Saved `proof.json` files under `proofs/TARGET/hoimin_budget_math_rlib/` are search
strategies, not successful-result caches. Choose the current target and copy its relative paths into
`verif/hoimin_budget_math_rlib/` after fresh generation, then add `--replay` to
the proof command while retaining `--no-cache -j 2`. Replay still invokes the
pinned solvers. Never reuse a cached successful answer as acceptance evidence.

Local production verification on the pinned native Linux ARM64 development
image generated 25 Coma files containing 59 named goals per target. Each target
passed both a first no-cache proof and a fresh-generation no-cache replay of its
saved strategy. The complete six-run sequence took 121.98 seconds with an empty
owned Cargo target directory at entry; installed tools and registry downloads
were already available. This is a cold **crate build**, not a cold environment
installation or the native Linux x86_64 CI acceptance measurement.

| Rust target | First generation / proof | Fresh warm generation / replay |
| --- | ---: | ---: |
| `x86_64-unknown-linux-gnu` | 20.65 s / 10.62 s | 0.31 s / 8.67 s |
| `aarch64-apple-darwin` | 15.79 s / 8.99 s | 0.21 s / 8.68 s |
| `x86_64-pc-windows-msvc` | 13.94 s / 9.00 s | 0.23 s / 8.73 s |

Peak process/child RSS was 1,490,344 KiB during generation and 272,072 KiB during
proof, rather than an aggregate environment measurement. The owned Linux build
directory used 187,644 KiB afterward. Every round checked unchanged input hashes.
Compiling mutations of ceiling, saturation, auto timeout and strict shortfall
each produced their corresponding unproved goal; omitting one projection field
from structural equality also failed its equality goal. All mutation sources
were restored before these final three-target runs. Stable Rust's all-features
workspace suite passed 2,916 tests with 33 ignored and no failures; Clippy passed.
