# Pinned Creusot environment

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
effective Why3find configuration. Task 3 and Task 4 must run the proof check
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
Task 4 must use its own source-freshness and proof-status gates.

On this Rancher Desktop host, a bind mount from `/tmp` into the Docker VM
failed because the VM did not see that source path. Streaming the script on
standard input worked. Task 3 can place task-owned source in a container via
standard input or `docker cp`; do not assume host `/tmp` is mountable. The
observed image interpreter is Python 3.12.3.
