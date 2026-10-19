# Task 2 report: pinned Linux verification environment

Base: `685e71aa867f6c54a7eb1b673451d005d14813ab`.

Status: local native ARM64 development environment built, preflighted and smoke
proved; native Linux x86_64 full-job acceptance remains Task 4.

## Scope and controller decisions

Created strict preflight, fixed tool manifests, isolated Docker recipe, installation
and configuration scripts, tests and environment documentation. The controller
recorded the approved local native ARM64 development variant in the durable plan
and task brief; Task 2 includes that plan change. Exact source versions and all
three required proof targets remain unchanged. Native Linux x86_64 cold/warm and
the complete setup-plus-proof 30-minute acceptance remain mandatory in Task 4.
No image is published; unrelated Docker images/containers/volumes/worktrees are
preserved. No product arithmetic, proof contracts or trusted/assume holes change.

The controller also approved OS-enforced solver limits: absolute GNU timeout
30 seconds and prlimit AS2GiB in the effective Why3 config. AS is stricter than
RSS; this does not establish a whole-environment RSS limit. Task 4 retains the
whole-job deadline, concurrency, fresh obligations and final proof-status gates.

## Fixed provenance

- Creusot v0.14.0 resolves to `65349025c67e69fcd7479f1f420fa5904f69bc27`.
  Source archive SHA256 `e68f15e09fbbddfc7d90edc2ca43ca69127e515c460b3eb39e01f7f8a6d8bb17`.
- Why3 `05bf4ddf55cb3042f34c3bc0ed3aff56a226978a`, source SHA256
  `fc84ae17de6a79ea253cb7df96afd8e4eae562a99b43379a4cf5d12efa701b92`.
- Why3find `0f054b93c86ac7ba20bca7df5d1563a9a4434a30`, source SHA256
  `7da871320d7c0dcf058dbb701f6fcff46d550ed4297b4a8c2c89f707ba39e1d9`.
- Rust nightly-2026-08-03, rustc `1.99.0-nightly (11177f223 2026-08-02)`,
  full compiler commit `11177f2235f0c842b00f82c558ad9480c0c3a895`.
- OCaml5.3.0 source SHA256
  `22c1dd9de21bf43b62d1909041fb5fad648905227bf69550a6a6bef31e654f38`.
- Opam repository snapshot `c9d86fa5bb8024f7acb88e1096c3a7f1f9dd22a9`, SHA256
  `dd17bf006a6edebea24c1881e8d89732c746847a79aadaae32b1819123962b18`.
- Z3 5.1.0 and CVC5 1.4.1 use the official Creusot upstream x86_64 and ARM64
  distribution URLs and SHA256 values. Manifests record every URL/hash, including
  all dated Rust components/standard libraries, immutable Ubuntu image digests,
  the signed apt snapshot and the checksum-verified dated CA bootstrap bundle.

Authoritative inputs were read from pinned Creusot installer/setup/CLI sources,
pinned Why3find CLI/runner/config-loading sources, Why3's actual configuration
loader and the dated official Rust distribution manifest. Sources are linked in
`formal/creusot/toolchain.json` and README; cached research sources are in
`/tmp/hoimin-issue999-design-mnk8miii/upstream`.

## RED / GREEN and normal verification

1. `.venv/bin/python -m pytest tests/test_creusot_environment.py -q` initially
   failed all15 cases because the checker did not exist. The implemented checker
   then passed15/15. Subsequent behavioral RED found a boolean accepted as schema
   version, relative receipt/tool paths, configuration environment drift and PATH
   selecting a different executable. Added each requirement before its fix.
2. Adding mandatory timeout/prlimit executables produced observed RED against the
   old required-tool set, then GREEN after implementation. Final focused run:
   `32 passed in 7.29s`.
3. Ruff format/check and `ty check tools/creusot_environment.py
   tests/test_creusot_environment.py`: pass. Shell `bash -n` and `ast.parse` of
   embedded installer/configuration Python: pass. Final checks rerun before commit.
4. Initial bare full Python run: `6 failed,1122 passed,11 skipped in38.97s`.
   Four resource-guard tests lacked process inspection under sandbox:
   `test_propagates_normal_child_exit`,
   `test_rejects_root_exit_that_leaves_a_descendant`,
   `test_terminates_process_group_at_rss_limit`,
   `test_terminates_process_group_at_timeout`.
   Two release tests lacked writable uv cache:
   `test_skipped_reservation_prevents_preparation`,
   `test_release_preparation_uses_reserved_tag`.
   With required `UV_CACHE_DIR=/tmp/hoimin-issue999-uv-cache`, venv PATH and
   process-inspection access, full rerun passed1130/11skip in41.01s.
5. Final full command after mandatory-tool coverage:
   `PATH="$PWD/.venv/bin:$PATH" UV_CACHE_DIR=/tmp/hoimin-issue999-uv-cache python -m pytest -q`
   passed `1132 passed,11 skipped in41.64s`.

Tests use controlled executable files and real subprocess/file operations, rather
than mocking command order. A wrong-version fixture has a matching receipt hash;
a changed binary prints the correct version. Nonzero exit with correct stdout,
missing tools, configuration hash/environment drift, PATH drift, stale/missing
receipts, absent executable receipt entries and malformed manifests all fail.

## Mutation testing

Used the already-built working-tree hoimin0.13.0 binary; no extra heavy Rust build.
Plans used one worker, max workspace8GiB, minimum free10GiB and macOS best-effort
memory acknowledgement. Scratch directories used immediate cleanup traps and
were removed exactly. The first verify stopped before baseline because its
relative `.venv/bin/python` was excluded from the worker copy. Cleanup was clean;
replanned with the absolute shared interpreter. No killed/survivor conclusion was
made from the stopped run.

Selected mutation: version rejection `or`→`and`. First passing run had fresh
baseline5858ms, mutant exit1 after6.231s, killed1/survived0, complete execution and
clean cleanup. After the helper refactor, replanned exact line171 and verified
candidate `m1_ae5d015d3b22013fd7a06936fcb64d5748670daeace209003dd48b6d64acb466`:
source hash `65bdacd823fb08a27c317e8b1d6d5106139b93b0a5c419945d93161b5aa4e02d`,
fresh baseline6775ms, killed1/survived0, complete, peak owned54336654bytes,
cleanup clean. The later required-tool-set addition leaves that predicate intact;
its two additional missing-tool cases and happy path pass separately.

## Actual bounded Linux limit checks

Read actual `timeout --help` / `prlimit --help` on the pinned ARM64 Ubuntu base:
`timeout (GNU coreutils) 9.4`, `prlimit from util-linux 2.39.3`.

Ran an isolated task-owned Linux container with actual primitives, retained at
`/tmp/hoimin-issue999-linux-limit-probe/{probe.sh,result.json}`:

```text
actual_as_kib=2097152
small_as_limit_exit=127
timeout_exit=137
descendant_state=gone
elapsed_seconds=30.580550416838378
probe exit=0
```

The AS2GiB child reported its real limit. A tiny AS1MiB `/bin/true` invocation
failed to map libc and returned nonzero, without allocating a large buffer. Exact
GNU timeout30s killed a bash process group with a sleeping descendant; its child
was gone afterward. These results establish the actual deadline/AS primitives,
not final proof-status or whole-environment memory guarantees. Config uses
absolute executable paths, whose content and actual versions preflight checks.

## Environment attempts, timing and measured limitations

Host: macOS ARM64. Docker: Linux aarch64,4CPUs,8321900544bytes RAM. Thirteen unrelated
containers remain running. Host initially had~96GiB free; Docker VM filesystem
was98GiB total/~32GiB free, so host free space alone overstated VM headroom.
Later native setup check: VM30GiB free, MemAvailable3453120KiB.

All timed builds used a hard1800-second watchdog, one heavy build, Cargo2 jobs and
opam2 jobs. Actual measurements are not native x86_64 CI acceptance. Research
source retrieval is separate from environment installation. No cap was raised.

- `docker run --platform linux/amd64 ... uname -m` returned `x86_64` on the ARM64
  engine, confirming emulated execution. Base pull for that probe was not timed
  independently; later build-only elapsed cannot represent complete cold setup.
- Emulated attempts1–3 failed during CA/apt bootstrap. Initial attempt elapsed
  7.898s, host disk delta+94208bytes; attempt2 elapsed5.600s, delta+282624bytes.
  No TLS/signature checks were disabled. Diagnostics showed remote ADD created
  non-traversable CA parent directories for `_apt`; fixed modes0755. Apt now
  fails on any index-fetch error. Original logs/JSON remain under
  `/tmp/hoimin-issue999-cold-context` and numbered variants.
- Emulated attempt4 completed signed OS snapshot setup110.8s, verified sources,
  rustc/cargo/rustc-dev artifacts, then spent several minutes installing rustc-dev.
  At controller request, only its task-owned Docker build client was interrupted:
  exit130, elapsed624.455869s, observed host disk delta−984100864bytes. Stage not
  completed, no Creusot/tool/proof success claimed. Negative host disk deltas are
  observed system-wide changes during other activity, not isolated image sizes.
- Native ARM64 attempt1 included base image setup; all hashes verified and real
  rustc metadata compilation succeeded separately for the three targets.
  OCaml5.3.0 built, then strict opam rejected a local directory source pin for
  missing checksum. Build exit1 / inner exit40 after324.107359s overall; tool-stage
  wall4:16.72, peak process RSS521824KiB. Fixed this with checksummed local archives
  in a local package repository; OPAMREQUIRECHECKSUMS remains true.
- Native attempt2 starts epoch1792371015.277737, deadline1792372815.277737;
  final recipe separates Rust/components, OCaml, proof tools and final config.
  Rust/components stage completed75.29s, peak process RSS158772KiB and exit0;
  all three actual target metadata compilations passed again. Remaining native
  build/preflight/ordinary proof-smoke and warm measurements are appended below.

Peak RSS from GNU time is a peak process/child measurement, not simultaneous
aggregate environment RSS. Shared VM/host disk deltas include other activity.
Task-owned image size and final VM headroom are recorded after successful build.
The native x86_64 cold/warm job must still include fresh setup and all three real
proof targets within30minutes in Task4. An ARM64 result or ordinary-crate smoke
cannot establish the production arithmetic proofs; those are Task3's scope.

## Self-reviews

Five implementation and five testing self-reviews plus follow-ups are durable in
`docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md`, covering source
provenance, effective config, runtime selection, isolation/cache/disk, architecture,
RED/GREEN, independent failure paths, mutation cleanup, whole Python suite and
actual Linux integration. This report preserves failures as well as passing runs.

## Final measured environment evidence

The final working native ARM64 warm build was
`/tmp/hoimin-issue999-native-warm-context-6`: exit 0 in 82.817 seconds, observed
host disk delta +146096128 bytes. Its tool layers were cached; the final
configuration stage took 5.05 seconds and peak process RSS was 145432 KiB.
Image export took 50.2 seconds and unpack 25.9 seconds. The resulting
`hoimin-issue999-creusot-arm64:local` image is Linux arm64, image ID
`sha256:aad2302c3aeddac574b38e91ae4c8d44cee551671722a7ac25e68a66846dee89`,
size 1297286234 bytes. Docker VM filesystem before ordinary-crate smoke was
105088212992 total, 74524188672 used, 25178591232 available bytes; afterward
it was 74524381184 used, 25178398720 available (an observed 192512-byte VM
delta, subject to unrelated activity). The 13 unrelated running containers
remained running. Strict `check` passed all 11 actual executables, their expected
versions, PATH/environment selection and receipt/configuration/runtime hashes.

Late setup repair history is preserved rather than folded into that successful
warm timing:

| Attempt | Exit / wall seconds | Observed host disk delta | Cause and repair |
| --- | ---: | ---: | --- |
| Native cold 1 | 1 / 324.107 | -939921408 B | OCaml built; strict opam rejected checksumless local directory pin. Replaced it with checksum-bearing local source archives. |
| Native cold 2 | 1 / 287.146 | +148606976 B | Local opam fetch required `rsync`; installed it from signed snapshot and corrected local-switch `_opam` paths. |
| Warm 3 | 1 / 67.206 | +41705472 B | Why3's parallel Makefile target raced Dune; used outer `make -j1` and `DUNEJOBS=2`, then separated Why3 and Creusot build layers. |
| Warm 4 | 1 / 116.544 | +37789696 B | Creusot release build succeeded, then direct prelude generator missed runtime `CARGO_MANIFEST_DIR`; used pinned upstream's `cargo run` invocation. |
| Warm 5 | 1 / 57.427 | +1354014720 B | Strict preflight found an outdated expected ARM64 CVC5 banner. Verified the actual ARM64 banner and the hashed AMD64 artifact's identical banner before correcting the manifest. |
| Warm 6 | 0 / 82.817 | +146096128 B | Final config/preflight succeeded; cached heavy layers, image export and unpack completed. |

All attempts had 1800-second watchdogs; none relaxed a source pin, proof target
or approved resource cap. Host disk deltas include unrelated system activity.
Attempt logs and `measurement.json` files remain in the named task-owned `/tmp`
directories. No final-recipe no-cache rebuild was run after repair, per
controller ruling; the complete native x86_64 cold/warm setup-plus-proof gate
still awaits Task 4.

An ephemeral ordinary crate created by `cargo creusot new smoke_crate` used its
generated `add_one` contract, the pinned local Creusot source's `creusot-std`
path, and the installed `why3find.json`. In a single native ARM64 container,
each target was generated sequentially with
`cargo creusot --only=coma -j 2 -- --target TARGET`, then the nonempty
`verif/smoke_crate_rlib/add_one.coma` was proved with
`cargo creusot --only=prove --no-cache -j 2 --why3find-arg=--goals
verif/smoke_crate_rlib/add_one.coma`. Every proof reported
`Goal vc_add_one: ✔ (1)` and `Proved ... ✔`, with exit 0. Each freshly
compiled target produced a 1002-byte Coma file with SHA256
`8f52b6e44ad235ef513f89339b605908be248c73d37c5a964a7e331ae45e0d7e`;
identical file hashes reflect this simple target-independent example, not a
claim that production proof obligations are target-independent.

| Target | First generation elapsed / peak process RSS | Fresh proof elapsed / peak process RSS | Warm Cargo invocation / no-cache proof elapsed |
| --- | ---: | ---: | ---: |
| `x86_64-unknown-linux-gnu` | 17.44 s / 1500452 KiB | 2.25 s / 271008 KiB | 0.03 s / 2.88 s |
| `aarch64-apple-darwin` | 15.42 s / 1495632 KiB | 2.42 s / 271628 KiB | 0.02 s / 2.47 s |
| `x86_64-pc-windows-msvc` | 14.83 s / 1492284 KiB | 3.57 s / 270944 KiB | 0.02 s / 2.25 s |

The first warm Cargo invocations reused their generated output (no `Checking`
line); the warm proofs still used `--no-cache` and returned the named goal.
For an observed freshness method, a second container run touched only the
ephemeral sample's `src/lib.rs` before each warm target invocation. Each then
printed `Checking smoke_crate`, regenerated the nonempty Coma file, and passed
its `--no-cache` proof:

| Target | Fresh warm generation elapsed / peak process RSS | Fresh warm proof elapsed / peak process RSS |
| --- | ---: | ---: |
| `x86_64-unknown-linux-gnu` | 0.20 s / 146108 KiB | 2.43 s / 271196 KiB |
| `aarch64-apple-darwin` | 0.14 s / 145836 KiB | 3.55 s / 271260 KiB |
| `x86_64-pc-windows-msvc` | 0.14 s / 142624 KiB | 2.39 s / 271196 KiB |

The complete scripts/logs are
`/tmp/hoimin-issue999-task2-smoke/{full-smoke,fresh-warm-smoke}.{sh,log}`;
the latter's tested script is preserved as `formal/creusot/smoke.sh` and run
via `docker run --rm -i --platform linux/arm64 --entrypoint /bin/bash
hoimin-issue999-creusot-arm64:local -s < formal/creusot/smoke.sh` from the
repository root. The container places the sample under `/tmp/smoke_crate`,
the pinned dependency under `/opt/creusot/src/creusot/creusot-65349025c67e69fcd7479f1f420fa5904f69bc27/creusot-std`,
the strict checker/manifest under `/recipe`, and generated obligations under
`/tmp/smoke_crate/verif`. It uses the container-local Cargo cache/target path;
no shared host build directory or other worktree is mounted. An attempted host
`/tmp` bind mount failed with Docker `bind source path does not exist`, so the
successful scripts were streamed over stdin into task-owned ephemeral
containers. `docker cp` into a task-owned container is an available alternative
for Task 3 source transfer, but was not exercised here. The image's actual
`python3 --version` is Python 3.12.3; Task 4 runner code must support it. These
numbers are process/child peak RSS, not aggregate container memory. A smoke
contract is not the Task 3 production arithmetic proof. Task 4 must regenerate
fresh obligations from current product source without touching product source
as a substitute freshness gate, and independently establish all
three proof runs, concurrency/failure gates and the complete native x86_64
30-minute job.

Final focused tests, lint/type/syntax checks, self-review and hook/commit
evidence follow below.

## Final checks and commit

- Focused checker: absolute shared Python interpreter, `python -m pytest
  tests/test_creusot_environment.py -q`: 32 passed in 7.32 seconds, exit 0.
- `ruff format --check` on the two changed Python files: 2 already formatted;
  `ruff check` and `ty check` on those files: all checks passed, exit 0.
- `bash -n formal/creusot/*.sh`, `ast.parse` on all embedded installer/config
  Python blocks, `git diff --cached --check`: exit 0.
- Cross-manifest audit: the x86_64 and ARM64 manifests have equal pinned
  Creusot/Why3/Why3find/opam sources and common Rust target-library artifacts,
  proof targets, effective configuration, environment, source URLs, OCaml
  compiler and tool names; architecture-specific host compiler/solver artifacts
  differ intentionally. Audit script exit 0 after correcting an audit-only
  mistaken lookup of `ocaml` under `artifacts` (it is under `ocaml_compiler`).
- Already recorded whole Python suite `1132 passed, 11 skipped`; no new
  suite-wide implementation risk arose from the final documentation and
  disposable smoke script, so the whole suite was not repeated. The selected
  mutation remains killed with clean cleanup as detailed above.
- Normal `git commit` hook ran, without bypass: all four Rust-only fmt/Clippy
  hooks reported `(no files to check) Skipped` because this commit changes no
  Rust/Cargo input. Python/shell checks above are the relevant gates.
- Final `git status --short` was empty. Commit:
  `336be05e62a3ec6cf47e4949d7e195804d79c177` (`Build pinned Creusot
  verification environment`).

The commit contains 17 changed files: the durable plan/review updates,
`formal/creusot/{Dockerfile,README.md,configure.sh,creusot_why3.conf,
install-components.sh,install-ocaml.sh,install-why3.sh,install.sh,smoke.sh,
toolchain-arm64.json,toolchain.json,why3.conf,why3find.json}`,
`tests/test_creusot_environment.py`, and `tools/creusot_environment.py`.
The task-owned SDD report and progress ledger are ignored by Git and preserved
in the worktree rather than included in the commit.

The five implementation and five testing self-review passes are in
`docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md`, with a
follow-up for the final native smoke/freshness and exact resource observations.
They covered pinned provenance, actual configuration selection, receipt/hash
and PATH drift, isolation and cache, architecture boundaries, observed
RED/GREEN failure cases, independent failure paths, mutation cleanup, suite
results and actual Linux integration. A final requirement-by-requirement check
found the Task 2 deliverables present. Open acceptance concern: the successful
ARM64 build reused repaired layers; no final-recipe native Linux x86_64 cold or
warm setup-plus-production-proofs measurement exists yet. That unchanged
30-minute/three-target mandatory gate belongs to Task 4; Task 3 owns the
production arithmetic contracts/proofs.

## Independent-review fix 1: effective project configuration

Base for this fix: `336be05e62a3ec6cf47e4949d7e195804d79c177`. The
reviewer found a real gap: the earlier preflight hashed only
`/opt/creusot/data/why3find.json`, while pinned cargo-creusot changes to the
project root before invoking Why3find, which selects the project-local
`why3find.json`. The installed file could remain pinned while the selected
project changed prover selection, time or tactics.

The revised interface separates the two legitimate checks:

```text
python3 /recipe/tools/creusot_environment.py check-installation --manifest /recipe/formal/creusot/toolchain.json
python3 /recipe/tools/creusot_environment.py check --manifest /recipe/formal/creusot/toolchain.json --project-root PROJECT
```

The first is the image's default self-check before a project exists. The
second is mandatory before proof in Task 3/4: it performs the same installation
check, requires `PROJECT/why3find.json`, and compares the actual file bytes'
SHA256 with manifest `project_config`, which names the pinned installed JSON
and its existing `files` hash. Omitting `--project-root`, passing a missing
project file, or changing any byte fails. The smoke passes its actual crate
root (`$PWD`). README and durable plan now state the distinction. Task 3/4
must use the proof-facing form on the workspace root that cargo-creusot will
use; installation-only is not a proof entry check.

Observed behavioral RED: after adding controlled project-local fixtures,
parameterized changes to `provers`, `time` and `tactics` plus removal of the
file all returned exit 0 on the old checker (`4 failed, 32 deselected`). A
separate two-case RED showed old `check` accepted no project root and lacked
`check-installation`. GREEN: focused `tests/test_creusot_environment.py`
passed 38/38 in 12.72 seconds. Ruff format/check, ty, bash syntax, embedded
installer/configuration Python syntax and `git diff --check` passed after one
Ruff formatting correction. The prior full Python suite and mutation result
are preserved; no unrelated full suite was repeated for this narrow interface
change.

For actual Linux validation, two ordinary `docker build` warm attempts from
different tar contexts unexpectedly missed the `install-components.sh` COPY
cache despite identical script content; host tar metadata differed (live file
gid20 vs previous context gid0). Only each task-owned Docker client was
interrupted (exit130) while the rust-components stage was downloading/installing
small inputs, at stage-log 26.24s and 12.63s respectively, before rustc-dev or
proof-tool compilation. Logs are
`/tmp/hoimin-issue999-project-config-fix-build.log` and
`/tmp/hoimin-issue999-project-config-fix-context/build.log`. No unrelated
container or prior image was removed. Instead, the local validation image was
derived from the already verified ARM64 image ID
`sha256:aad2302c3aeddac574b38e91ae4c8d44cee551671722a7ac25e68a66846dee89`.
Its local-only overlay copied the same final-stage formal files, selected
manifest and checker, ran the same `configure.sh`, inherited the original
final-stage PATH/environment, set `/workspace` and used the new
`check-installation` CMD. Tool-source layers were reused without rebuilding.
Overlay recipe/log: `/tmp/hoimin-issue999-project-config-fix-context/{Dockerfile.overlay,overlay-build-final.log}`.
The configuration stage took 3.51s, peak process RSS143020KiB, exit0. The
resulting unpublished Linux ARM64 image is
`hoimin-issue999-creusot-arm64:fix1-local`, ID
`sha256:f1eec1050faa3c0af5e540eabc114c165c308abfe4fcdc585e3437b1a23e8603`,
size1297357694bytes. The old `:local` tag remains the pre-fix image.

The updated overlay's default installation check exited0 for all 11 actual
executables. A separate task-owned project-check script copied the pinned JSON
to `/tmp/project-check/why3find.json`; proof preflight exited0, then changing
its `time` to99.0 produced `project configuration hash mismatch` and exit1;
removing it produced `missing project configuration` and exit1. Script/log:
`/tmp/hoimin-issue999-project-config-fix-context/{project-check.sh,project-check.log}`.
The revised committed `formal/creusot/smoke.sh`, streamed into the overlay
image over stdin, checked its actual project and completed six generation/proof
pairs: first and source-triggered warm generations for each of
`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` and
`x86_64-pc-windows-msvc`. Each run generated nonempty 1002-byte Coma with
the same sample SHA256 recorded above; each fresh `--no-cache -j 2` proof
reported `Goal vc_add_one: ✔ (1)` and exit0. First generation/proof seconds
were 19.34/2.48, 12.65/3.50 and 14.16/3.36 respectively; fresh-warm pairs
were 0.13/2.35, 0.14/2.27 and 0.10/3.48 seconds. Complete output:
`/tmp/hoimin-issue999-project-config-fix-context/smoke.log`.

VM filesystem before the overlay fix was 105088212992 total,
74569371648 used and25133408256 available bytes; afterward 74567770112
used and25135009792 available. The observed negative disk delta includes
unrelated activity and is not an image-size measurement. Thirteen unrelated
containers remained running. This local overlay confirms the changed final
interface and ordinary-crate proof smoke; it is not a final-recipe native
x86_64 cold/warm setup or production proof acceptance. Those unchanged gates
remain in Task 4 and Task 3 respectively.

Final fix checks: focused checker `38 passed in 12.78s`; Ruff format/check and
ty on changed Python files passed; shell syntax, embedded Python syntax,
manifest-selected configuration hash consistency and staged whitespace check
passed. The normal pre-commit hook ran without bypass; all four Rust-only
fmt/Clippy hooks skipped because no Rust/Cargo input changed. The fix committed
as `09226fa8` (`Check selected Creusot project prover configuration`), on top
of Task 2's original `336be05e62a3ec6cf47e4949d7e195804d79c177`. The
review-fix commit changes the plan, durable review, Dockerfile/README,
configure/smoke scripts, both manifests, checker and focused tests. No product
arithmetic or solver limit changed.
