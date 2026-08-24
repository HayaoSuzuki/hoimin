# Rust Toolchain Reproducibility and Async Setup Design

## Problem

The push of `6682a2cfc9af04eb1693dca7d2500899d1a7245a` failed before any Rust test
job ran. All three quality jobs installed the moving `stable` channel, received
Rust 1.98.0, and failed on the new `clippy::unused_async_trait_impl` lint. The
same source passed the quality gate on Rust 1.97.1. A required CI result can
therefore change without a source or dependency change.

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

- Make required CI and release builds use one repository-pinned stable Rust
  toolchain.
- Keep current-stable compatibility visible without letting an unreviewed
  toolchain release change required results underneath an unchanged commit.
- Pass the Rust 1.98 quality gate without lint suppression, dummy awaits, or a
  downgrade to Rust 1.97.
- Preserve the public `ShellContext::new(...).await` call contract while making
  its asynchronous boundary real.
- Keep caller-provided output writers out of Tokio's blocking pool so the
  public API gains no `Send + 'static` requirement.
- Preserve the declared Rust 1.88 MSRV and the pinned randomized-test nightly.
- Keep CI/toolchain repair separate from any Windows flaky-test change.

## Non-goals

- Changing CLI behavior, configuration, report formats, or process limits.
- Starting the run timeout before shell setup completes.
- Making shell setup cooperatively cancellable.
- Changing the existing preflight blocking-I/O state-machine design.
- Fixing or weakening a Windows test in this branch.
- Suppressing `clippy::unused_async_trait_impl` locally or globally.
- Automatically merging toolchain updates.

## Toolchain contract

The repository gains a root `rust-toolchain.toml` with an exact `1.98.0`
channel, the minimal profile, and the `clippy` and `rustfmt` components. This is
the single stable development and required-CI toolchain declaration. Normal
`cargo` and `rustup` commands run from the repository resolve that override
without a mutable channel name, unless a developer deliberately supplies a
higher-precedence command-line selector or `RUSTUP_TOOLCHAIN` environment
override.

Required jobs in `.github/workflows/ci.yml` install the active repository
toolchain with `rustup toolchain install`; they do not name `stable` and do not
change the user's default toolchain. Commands continue to omit a `+toolchain`
selector and therefore use the repository override. The pinned
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

The two compatibility lanes remain explicit exceptions:

- the MSRV job installs and invokes `+1.88`, matching
  `workspace.package.rust-version`;
- the randomized-order job installs and invokes
  `+nightly-2026-07-27`.

The development guide distinguishes the pinned current toolchain from the
MSRV. Updating the pinned compiler is a reviewed maintenance change: update
`rust-toolchain.toml`, resolve its diagnostics, and run the complete required
gate in one pull request. Raising the MSRV remains a separate decision and
continues to require the existing manifest, workflow, test, and documentation
updates.

## Latest-stable canary

A separate `.github/workflows/rust-stable-canary.yml` runs at 03:00 UTC every
Monday (`0 3 * * 1`) and on `workflow_dispatch`. It has read-only contents
permission and runs on Ubuntu. It uses the same commit-pinned checkout,
Python 3.14, and uv setup actions as required CI, runs `uv sync --frozen`, and
installs `stable` with the minimal profile plus the `rustfmt` and `clippy`
components. The validation commands select the canary toolchain explicitly:

- `cargo +stable fmt --all -- --check`;
- `cargo +stable clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo +stable test --workspace`.

The Python environment is not incidental: workspace tests exercise Python-backed
paths, so a canary that omitted the required CI setup would measure a different
system. The canary is not triggered by pull requests or pushes and is not a
dependency of required CI jobs. It is also not added to branch protection. A new
Rust release can therefore produce a visible failed canary without retroactively
making an unchanged main commit fail its required checks. The resolution is a
normal toolchain-update pull request, not a retry or an emergency downgrade.

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
  `RUSTUP_TOOLCHAIN` environment override in required CI;
- require ordinary jobs to use the repository toolchain while preserving the
  explicit MSRV and nightly selectors, and reject an unnecessary `+stable`
  selector that would bypass the exact repository pin;
- require every CI job to be classified as either a repository-toolchain job
  or one of the two explicit compatibility lanes, and require repository
  toolchain installation to precede its first Rust or maturin command;
- parse the canary workflow and require only `schedule` and
  `workflow_dispatch` triggers, read-only permissions, commit-pinned setup
  actions, Python and uv environment parity, the required stable components,
  explicit `+stable` commands, and no dependency from required CI;
- preserve the release workflow's exact artifact-only contract and reject a
  release-specific Rust selector that could override the repository pin;
- require the development guide to describe deliberate pinned-toolchain
  updates separately from MSRV updates and to keep its opening local-gate
  command block exactly aligned with the primary repository-toolchain and
  wheel-gate commands; the MSRV and nightly commands remain in their separate
  compatibility sections.

These tests prevent the repository declaration, required workflow, canary, and
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

Required CI must then pass on Ubuntu, Windows, and macOS using the pinned
toolchain. The canary workflow is validated structurally in this change; its
first scheduled or manually dispatched result is follow-up evidence rather
than a prerequisite for merging the required-CI repair.

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
`resource/windows.rs`, `resource/suspended.rs`, `run_e2e.rs`, Windows timeout
values, retries, or test scheduling.

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

This makes the cited run green but leaves required results dependent on release
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
