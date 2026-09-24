# Rust Toolchain Reproducibility and Async Setup Design

## Problem

The push of `6682a2cfc9af04eb1693dca7d2500899d1a7245a` failed before any Rust test
job ran. All three quality jobs installed the moving `stable` channel, received
Rust 1.98.0, and failed on the new `clippy::unused_async_trait_impl` lint. The
[cited macOS job](https://github.com/tokyogas-tech/hoimin/actions/runs/32644139281/job/97205635658)
records the same commit, Rust 1.98.0, and exactly the three diagnostics described
below. The same source passed the quality gate on Rust 1.97.1. An ordinary CI
result can therefore change without a source or dependency change.

The new diagnostic also exposes two different code conditions that must not be
handled as one mechanical lint edit:

- `ShellContext::new` is an async public constructor whose body performs only
  synchronous setup, including temporary-file creation and platform resource
  backend initialization.
- `FirstWriteFails` is a test double implementing an async trait with operations
  that intentionally complete immediately.

The current Windows-flake work was started from this red main baseline and
mixed an unrelated Job Object production hypothesis into the same branch. That
work is not evidence for the Rust 1.98 failure and is excluded from this design.

## Goals

- Make ordinary stable CI and release builds use one repository-pinned stable Rust
  toolchain.
- Keep current-stable compatibility visible without letting an unreviewed
  toolchain release change ordinary stable results underneath an unchanged
  commit.
- Pass the Rust 1.98 quality gate without lint suppression, dummy awaits, or a
  downgrade to Rust 1.97.
- Preserve the public `ShellContext::new(...).await` call contract while making
  its asynchronous boundary real.
- Keep caller-provided output writers out of Tokio's blocking pool so the
  public API gains no `Send + 'static` requirement.
- Preserve the declared Rust 1.88 MSRV and the pinned randomized-test nightly.
- Keep CI/toolchain repair separate from Windows process/resource production
  changes. A test-fixture-only portability repair is allowed when it is needed
  to run the unfiltered Windows baseline.

## Non-goals

- Changing CLI behavior, configuration, report formats, or process limits.
- Starting the run timeout before shell setup completes.
- Making shell setup cooperatively cancellable.
- Changing the existing preflight blocking-I/O state-machine design.
- Changing Windows process/resource production behavior or weakening a Windows
  assertion. The root-path fixture may use a junction when symlink creation is
  unavailable, while exercising the same linked-parent rejection invariant.
  Real-process integration fixtures may use one absolute six-second test
  deadline so readiness is established before cancellation, close, or
  classification is triggered; production timeout and cleanup semantics remain
  unchanged. The locked-session E2E fixture may select only the arithmetic
  operator needed by its scenario and measure its test deadline from the
  observed `mutant_started` event; its five-second production total timeout and
  shutdown behavior remain unchanged.
- Suppressing `clippy::unused_async_trait_impl` locally or globally.
- Automatically merging toolchain updates.

## Toolchain contract

The repository gains a root `rust-toolchain.toml` with an exact `1.98.0`
channel, the minimal profile, and the `clippy` and `rustfmt` components. This is
the single stable development and ordinary-CI toolchain declaration. Normal
`cargo` and `rustup` commands run from the repository resolve that override
without a mutable channel name, unless a developer deliberately supplies a
higher-precedence command-line selector or `RUSTUP_TOOLCHAIN` environment
override.

Every repository-toolchain job in `.github/workflows/ci.yml` installs the
active repository toolchain with `rustup toolchain install`; they do not name
`stable` and do not change the user's default toolchain. Commands continue to
omit a `+toolchain` selector and therefore use the repository override. The pinned
`maturin-action` resolves the nearest `rust-toolchain.toml` before installing
Rust, so release builds use the same channel without changing the release
workflow shape. At the pinned
`e83996d129638aa358a18fbd1dfb82f0b0fb5d3b` revision,
[`getRustToolchain`](https://github.com/PyO3/maturin-action/blob/e83996d129638aa358a18fbd1dfb82f0b0fb5d3b/src/index.ts#L350-L394)
walks from the manifest directory to the workspace root and reads the channel
from `rust-toolchain.toml`; both
[`dockerBuild`](https://github.com/PyO3/maturin-action/blob/e83996d129638aa358a18fbd1dfb82f0b0fb5d3b/src/index.ts#L599-L606)
and
[`hostBuild`](https://github.com/PyO3/maturin-action/blob/e83996d129638aa358a18fbd1dfb82f0b0fb5d3b/src/index.ts#L986-L1004)
consume that result. Any future action-revision update must reverify those
three paths in the pinned source before changing the workflow contract. A
release tag therefore cannot silently select a newer compiler than the commit
was tested with.

In this document, a *repository-toolchain job* means a job in `ci.yml` that uses
the ordinary stable compiler rather than the explicit MSRV or nightly selector.
That set includes the event-conditional cgroup job. It does not mean that every
such job is configured as a GitHub branch-protection required check; branch
protection and repository rulesets are external state and are not inferred from
workflow YAML.

The two compatibility lanes remain explicit exceptions:

- the MSRV job installs and invokes `+1.88`, matching
  `workspace.package.rust-version`;
- the randomized-order job installs and invokes
  `+nightly-2026-07-27`.

The development guide distinguishes the pinned current toolchain from the
MSRV. Updating the pinned compiler is a reviewed maintenance change: update
`rust-toolchain.toml`, resolve its diagnostics, and run the complete ordinary
gate in one pull request. Raising the MSRV remains a separate decision and
continues to require the existing manifest, workflow, test, and documentation
updates.

## Latest-stable canary

A separate `.github/workflows/rust-stable-canary.yml` runs at 03:00 UTC every
Monday (`0 3 * * 1`) and on `workflow_dispatch`. It has read-only contents
permission and runs on Ubuntu. It uses the same commit-pinned checkout,
Python 3.14, and uv setup actions as the ordinary test jobs, runs
`uv sync --frozen`, and installs `stable` with the minimal profile plus the
`rustfmt` and `clippy` components. The validation commands select the canary
toolchain explicitly:

- `cargo +stable fmt --all -- --check`;
- `cargo +stable clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo +stable test --workspace`.

The Python environment is not incidental: workspace tests exercise Python-backed
paths, so a canary that omitted that setup would measure a different system. The
canary is not triggered by pull requests or pushes and is not a dependency of
any `ci.yml` job. The workflow design does not require its check name in branch
protection; because that setting is external to this diff, it must be reviewed
separately if repository rules change. A new Rust release can therefore produce
a visible failed canary without changing the result of an unchanged ordinary-CI
commit. The resolution is a normal toolchain-update pull request, not a retry or
an emergency downgrade.

## Shell setup boundary

`ShellContext::new` remains async and retains its existing arguments and result.
Its synchronous infrastructure construction moves into a non-generic
`PreparedShellSetup` path. The preparation owns only data that can safely cross
Tokio's blocking-task boundary:

- an owned clone of `RunConfig`;
- the temporary spool directory and derived paths;
- the workspace handler;
- the platform resource backend;
- the process and analyzer handlers;
- a non-generic `PreparedReport` containing the output format and its initialized
  JSON report state, when required.

The async constructor clones the configuration before crossing the task
boundary, submits preparation through a small shell-setup helper backed by
`tokio::task::spawn_blocking`, and awaits its join result. After the prepared
value returns to the async task, the constructor attaches the caller-provided
`stdout` and `stderr` to `PreparedReport` and then returns `ShellContext`.

This split is required because `run_with_io` accepts borrowed writers that are
not required to be `Send` or `'static`. Moving the complete generic
`ShellContext` into `spawn_blocking` would impose a new public bound and is
rejected. Leaving `ReportHandler::new` to open its JSON temporary file after the
await would retain blocking filesystem I/O on the async runtime and is also
rejected. `PreparedReport` encodes the format and JSON-state invariant together;
it cannot represent JSON output without initialized JSON state. The existing
public `ReportHandler::new` remains available and delegates to the same prepare
then attach operations, while shell setup performs the prepare operation in the
blocking phase and retains the writers on the calling task.

Setup remains outside the run's total-timeout interval, matching current
behavior: `run_loop_prepared` establishes its deadline only after
`ShellContext::new` returns. The awaited blocking task keeps the Tokio executor
responsive. Setup consists only of bounded initialization calls that must finish
on their own; Tokio cannot abort `spawn_blocking` work after it starts, and
runtime shutdown may wait for that work. Run-control cancellation does not apply
because the interrupt monitor and run state do not yet exist. If the constructor
future itself is dropped, its join handle is detached, but the blocking closure
continues to own and eventually drop any partially constructed resources. Making
startup cooperatively cancellable would change resource ownership and requires a
separate design.

## Setup failure and ownership

Ordinary preparation errors retain their existing user-facing messages.
Failure before returning `PreparedShellSetup` drops every temporary file,
handle, and directory owned by the blocking task. A join failure is converted
into a distinct shell-setup join error; it is not mislabeled as an effect failure
because no run effect exists yet. Once blocking preparation has started, the
normal join failure is a panic. Cancellation is possible only while a queued
blocking task has not started or while the Tokio runtime is shutting down; the
design does not claim that an in-flight blocking operation can be aborted.

Once preparation succeeds, ownership moves exactly once into `ShellContext`.
No temporary path is reconstructed and no resource backend is created a second
time on the async task. Existing drop and close behavior begins after context
construction and remains unchanged.

## Async trait test double

The production `OutputSink` trait and `FileOutputSink` remain async because file
operations await Tokio I/O. The test-only `FirstWriteFails` implementation has
no asynchronous work. Its methods use non-async trait-implementation syntax,
following the Rust 1.98 Clippy contract.

The write error is constructed when `write_ring` is called and returned by a
`std::future::Ready` future. `finalize` returns a `std::future::poll_fn` future
whose poll operation is the invariant panic. This preserves the existing
deferred-on-poll behavior without declaring the trait method itself async and
without triggering `clippy::manual_async_fn`, which an `async` block wrapper
would trigger under Rust 1.98. Tests continue to prove that the collector stops
using the failed sink and never finalizes it. No production trait abstraction
changes for the sake of one test double.

## Executable workflow contracts

`tests/test_ci_workflow.py` extends its current MSRV and nightly checks with
stable-toolchain contracts:

- parse `rust-toolchain.toml` and require the exact channel, profile, and
  components;
- reject mutable `stable` installation, `rustup default`, `rustup override`,
  `rustup run`, `rustup update`, an alternate toolchain setup action, or a
  `RUSTUP_TOOLCHAIN` environment override in repository-toolchain CI jobs;
- require ordinary jobs to use the repository toolchain while preserving the
  explicit MSRV and nightly selectors, and reject an unnecessary `+stable`
  selector that would bypass the exact repository pin;
- require every CI job to be classified as either a repository-toolchain job
  or one of the two explicit compatibility lanes, and require repository
  toolchain installation to precede its first Rust or maturin command;
- parse the canary workflow and require only `schedule` and
  `workflow_dispatch` triggers, read-only permissions, commit-pinned setup
  actions, Python and uv environment parity, the required stable components,
  explicit `+stable` commands, and no dependency from `ci.yml`;
- preserve the release workflow's exact artifact-only contract and reject a
  release-specific Rust selector that could override the repository pin;
- require the development guide to describe deliberate pinned-toolchain
  updates separately from MSRV updates and to keep its opening local-gate
  command block exactly aligned with the primary repository-toolchain and
  wheel-gate commands; the MSRV and nightly commands remain in their separate
  compatibility sections.

These tests prevent the repository declaration, ordinary CI workflow, canary, and
documentation from drifting independently.

## Test strategy

The initial RED evidence is the exact Rust 1.98 command:

```text
cargo +1.98.0 clippy --workspace --all-targets --all-features -- -D warnings
```

It currently fails on `ShellContext::new` and the two `FirstWriteFails` methods.
The implementation must make that same command pass without an allow or expect
for `unused_async_trait_impl`.

The real shell-preparation path accepts a test-only before-start hook, following
the existing preflight pause-controller pattern. On a current-thread Tokio
runtime, that hook signals entry and blocks on a test gate while an independent
heartbeat future runs. A controller OS thread waits a bounded interval for the
heartbeat and always releases the gate afterward. The test asserts that the
heartbeat arrived before release and that real context preparation completed.
This avoids deadlock in the counterfactual: if preparation runs inline, the
controller times out, releases the gate, and the assertion fails. A total Tokio
timeout bounds the regression itself.

The production constructor uses the same preparation function with a no-op hook;
the test does not exercise an unrelated generic blocking helper. Existing
constructor tests continue to prove that creating a context does not create a
missing project root or session database. Report-handler tests cover both the
existing public constructor and prepared-state attachment for JSON, JSON Lines,
and human formats. Output-collection tests continue to prove the first write
error is retained and the failed sink is not finalized.

A compile-time characterization test constructs the public constructor future
with writers that borrow local buffers and contain `Rc`, making them both
non-`'static` and non-`Send`. The future is deliberately not polled. This test
passes before the refactor and must continue to compile afterward, so moving
the writers into `spawn_blocking` or adding either bound is detected directly
rather than inferred from `Vec<u8>` call sites.

Final local verification includes:

```text
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.88 check --workspace --all-targets --all-features --locked
cargo +1.98.0 test --workspace
cargo +1.98.0 test -p hoimin-cli --test run_e2e
cargo +1.98.0 test -p hoimin-core --features contracts
cargo +1.98.0 test -p hoimin-cli --features contracts
rustup toolchain install nightly-2026-07-27 --profile minimal
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uvx maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

On this Windows host, the complete `cargo +1.88 check --workspace
--all-targets --all-features --locked` command reaches an unrelated pre-existing
Windows-only `if let` guard in `resource/windows.rs` and fails with E0658 under
Rust 1.88. The changed async-trait implementation syntax is therefore checked
separately with Rust 1.88 locally; the complete MSRV command remains required
on the Ubuntu MSRV CI lane. This limitation must be reported as such, not
converted into a local PASS and not repaired by changing Windows production
code in this branch.

The hosted quality and test matrices must then pass on Ubuntu, Windows, and
macOS using the pinned toolchain before cross-platform success is claimed.
Local Windows verification cannot establish those hosted results. The
event-conditional self-hosted cgroup job is covered by workflow structure and
its next eligible main-push run, not by pretending it ran on a pull request.
Likewise, the tag-only release action is supported before a release by its exact
workflow contract, the pinned action-source audit above, and local wheel smoke;
an actual tag build remains release evidence. The canary workflow is validated
structurally in this change, while its first scheduled or manually dispatched
result is follow-up evidence rather than a prerequisite for merging the repair.

Focused generated mutation testing covers the two new, uniquely named
production setup helpers, `prepare_shell_setup` and
`prepare_shell_setup_sync`. The runner resolves a requested method by its
terminal name, so adding the generic names `new` or `attach` would select many
unrelated methods and would not be focused evidence. `PreparedReport` and the
constructor are instead covered by the format-state, public report integration,
non-`Send` writer, and constructor regressions above. In addition, a manual
counterfactual that executes setup inline instead of through `spawn_blocking`
must be detected by the current-thread heartbeat regression because generated
expression mutations are not required to remove that structural boundary.
Workflow and test-only changes have no Rust production mutation requirement.

## Change isolation and sequencing

This design is implemented on `fix/rust-1.98-ci`, created from `origin/main`
without tracking `origin/main`. Its diff must not contain changes to
`resource/windows.rs`, `resource/suspended.rs`, Windows timeout values, retries,
or test scheduling. Five reviewed implementation-time exceptions are included:

- `workspace/root.rs` changes only the Windows test fixture, falling back from
  a directory symlink to a junction when error 1314 shows that the host lacks
  symlink privilege; the same linked-parent rejection assertion remains active;
- `tests/run_e2e.rs` changes two `ok().is_some_and(...)` expressions to
  `is_ok_and(...)` for Rust 1.98 Clippy. Its locked-session timeout fixture also
  selects only `binary_add_sub`, records the absolute assertion deadline when
  `mutant_started` is observed, and verifies that the hoimin child is still
  running when the session lock is retained. The production five-second total
  timeout, retry policy, shutdown logic, and test scheduling are unchanged.
- `tests/lean_shutdown_oracle.rs` changes the same expression shape once in
  Unix-only process-liveness test code. The Ubuntu Rust 1.98 Clippy job exposed
  this target-specific occurrence after the Windows-local gate passed.
- `tests/process_handler.rs` gives real-process fixtures one absolute six-second
  test deadline. Operations controlled by the test wait for the child PID
  readiness event before firing. Timeout fixtures use a five-second process
  budget inside that deadline so Python startup is a precondition rather than
  an accidental one-second race; the asserted 900ms cleanup bound is unchanged.
  Its legacy relative wait helper is gated to Linux, matching its two cgroup
  callers, rather than compiled as unused code on macOS.
- `tests/test_ranked_plan_docs.py` reads the UTF-8 README explicitly instead of
  depending on the Windows ANSI code page, and `tests/test_wheel_smoke.py`
  distinguishes the development guide's new `uvx maturin` command from the
  unchanged README command.

None of these exceptions implements the earlier Job Object hypothesis or
changes Windows runtime behavior.

The abandoned uncommitted Windows stabilization spec, plan, and code are not
carried into this branch. After this prerequisite is merged and main is green,
Windows flakes are re-investigated from their own failing job logs on a fresh
feature branch. A process-handle fallback is considered only after a real
production-path reproducer demonstrates that the existing completion-port and
aggregate-notification paths are insufficient.

## Alternatives rejected

### Pin Rust 1.97.1

This restores the previous result by avoiding the compiler that exposed the
problem. It neither validates current Rust nor repairs the async boundary.

### Fix the three diagnostics but keep rolling `stable`

This makes the cited run green but leaves ordinary stable results dependent on release
date. Another lint, formatter, or compiler change can break unchanged main in
the same way.

### Add lint allowances or dummy awaits

These preserve an async function with no async boundary and encode no runtime
property. They contradict the workspace policy that exceptions be narrow and
domain-justified.

### Make `ShellContext::new` synchronous

This is honest about the current body but changes a public call contract and
still executes filesystem and platform setup on the async caller's thread.

### Move the complete generic context into `spawn_blocking`

This requires caller-provided writers to be `Send + 'static`, breaking the
existing borrowed-writer API. Preparing only the non-generic infrastructure
preserves the boundary.

### Keep the Windows changes in the prerequisite branch

They address a different failure family and were designed before the main
quality gate was understood. Combining them prevents independent review and
makes a passing result ambiguous.

## 2026-09-24 amendment: validate pin shape, not a copied version

PR #578's Rust 1.98.1 update exposed a redundant `1.98.0` equality in the
workflow contract tests. The exact stable version remains pinned in
`rust-toolchain.toml`; tests now validate a canonical numeric
`major.minor.patch` declaration, minimal profile, and clippy/rustfmt components.
They reject floating channels, incomplete versions and prerelease/suffixed
values without duplicating the current release. Future exact release updates
still run all compatibility gates. Package/wheel version equality and MSRV
remain separate contracts.

The repair uses an independent branch and includes the 1.98.1 update. Its
[implementation plan and review evidence](../plans/2026-09-24-rust-toolchain-smoke.md)
record the original failure and subsequent checks; the earlier measurements
in this document remain historical evidence.

## 2026-09-24 amendment: Action pins and structural workflow contracts

PRs #579/#580/#581 expose the same duplication in action SHA expectations.
The workflow files own exact commit pins. Tests validate a full 40-character
hexadecimal SHA before comparing each step's action identity independently of
the revision. Other step fields and the artifact-only release structure remain
exact; changes to repository identity, inputs, permissions, scripts, shells or
environments are not hidden by reference normalization. Raw automatic/manual
step comparisons still require matching versions where the job contract calls
for identical steps.

The combined repair updates cache to v4.3.0, checkout to v6.1.0 and setup-uv to
v8.3.2, including canary and boundary jobs omitted from the original proposals.
Only those dependency revisions change. The [combined implementation plan](
../plans/2026-09-24-actions-update-contracts.md) records reviews and test results.
These structural tests do not prove the runtime behavior of third-party code;
the actual GitHub jobs provide that execution evidence.

## 2026-09-24 amendment: Maturin release version contract

PR #583 still fails after taking current main because the release fixture repeats
the old maturin version. The workflow owns that exact pin, just as it owns Action
revisions. Release contract assertions validate a canonical `vMAJOR.MINOR.PATCH`
string and require the Windows/Linux builds to agree, then normalize only the
`maturin-version` input of `PyO3/maturin-action`. Other inputs, including build
arguments and target platforms, remain exact. Missing/floating versions,
inconsistent platform versions and unrelated input changes remain failures.

The repair updates both release jobs to maturin v1.15.0 without changing the
compatible range in pyproject.toml or the lockfile. The [implementation plan](
../plans/2026-09-24-maturin-release-contract.md) records the reproduction, review
and platform-specific validation. A local build does not execute the hosted
Windows/Linux release jobs or publish artifacts.
