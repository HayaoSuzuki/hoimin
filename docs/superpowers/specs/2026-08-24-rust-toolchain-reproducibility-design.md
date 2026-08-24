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
without a mutable channel name.

Required jobs in `.github/workflows/ci.yml` install the active repository
toolchain with `rustup toolchain install`; they do not name `stable` and do not
change the user's default toolchain. Commands continue to omit a `+toolchain`
selector and therefore use the repository override. The pinned
`maturin-action` resolves the nearest `rust-toolchain.toml` before installing
Rust, so release builds use the same channel without changing the release
workflow shape. A release tag therefore cannot silently select a newer compiler
than the commit was tested with.

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
permission, runs on Ubuntu, installs `stable`, and invokes commands explicitly
with `+stable`:

- `cargo +stable fmt --all -- --check`;
- `cargo +stable clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo +stable test --workspace`.

The canary is not triggered by pull requests or pushes and is not a dependency
of required CI jobs. A new Rust release can therefore produce a visible failed
canary without retroactively making an unchanged main commit fail its required
checks. The resolution is a normal toolchain-update pull request, not a retry
or an emergency downgrade.

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
- a prepared report spool for JSON output, when required.

The async constructor submits that operation through a small shell-setup helper
backed by `tokio::task::spawn_blocking` and awaits its join result. After the
prepared value returns to the async task, the constructor combines it with the
caller-provided `stdout` and `stderr` to build `ReportHandler` and then returns
`ShellContext`.

This split is required because `run_with_io` accepts borrowed writers that are
not required to be `Send` or `'static`. Moving the complete generic
`ShellContext` into `spawn_blocking` would impose a new public bound and is
rejected. Leaving `ReportHandler::new` to open its JSON temporary file after the
await would retain blocking filesystem I/O on the async runtime and is also
rejected. Report preparation therefore creates the optional spool in the
blocking phase, while report composition retains the writers on the calling
task.

Setup remains outside the run's total-timeout interval, matching current
behavior: `run_loop_prepared` establishes its deadline only after
`ShellContext::new` returns. The awaited blocking task keeps the Tokio executor
responsive, but setup is not abandoned on cancellation because the interrupt
monitor and run state do not yet exist. Adding cancellation at this boundary
would change startup and resource-ownership semantics and requires a separate
design.

## Setup failure and ownership

Ordinary preparation errors retain their existing user-facing messages.
Failure before returning `PreparedShellSetup` drops every temporary file,
handle, and directory owned by the blocking task. A panic or cancellation of
the blocking-task wrapper is converted into a distinct shell-setup join error;
it is not mislabeled as an effect failure because no run effect exists yet.

Once preparation succeeds, ownership moves exactly once into `ShellContext`.
No temporary path is reconstructed and no resource backend is created a second
time on the async task. Existing drop and close behavior begins after context
construction and remains unchanged.

## Async trait test double

The production `OutputSink` trait and `FileOutputSink` remain async because file
operations await Tokio I/O. The test-only `FirstWriteFails` implementation has
no asynchronous work. Its methods use non-async trait-implementation syntax,
following the Rust 1.98 Clippy contract.

The write error is constructed when `write_ring` is called and returned by the
`std::future::Ready` future. `finalize` panics immediately if it is called after
the write failure; its callers already await the call immediately, and the
test's contract is that the call never occurs. Tests continue to prove that the
collector stops using the failed sink and never finalizes it. No production
trait abstraction changes for the sake of one test double.

## Executable workflow contracts

`tests/test_ci_workflow.py` extends its current MSRV and nightly checks with
stable-toolchain contracts:

- parse `rust-toolchain.toml` and require the exact channel, profile, and
  components;
- reject mutable `stable` installation or `rustup default` in required CI;
- require ordinary jobs to use the repository toolchain while preserving the
  explicit MSRV and nightly selectors;
- parse the canary workflow and require only `schedule` and
  `workflow_dispatch` triggers, read-only permissions, explicit `+stable`
  commands, and no dependency from required CI;
- require the development guide to describe deliberate pinned-toolchain
  updates separately from MSRV updates.

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

The shell-setup helper receives a deterministic test operation. On a
current-thread Tokio runtime, the operation blocks on a test gate while an
independent heartbeat future runs. The heartbeat must complete before the gate
is released, proving setup was submitted to the blocking pool. The test then
releases the operation and checks its value or error. A counterfactual that
executes the operation inline must fail this test.

Existing constructor tests continue to prove that creating a context does not
create a missing project root or session database. Output-collection tests
continue to prove the first write error is retained and the failed sink is not
finalized.

Final local verification includes:

```text
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.88 check --workspace --all-targets --all-features --locked
cargo +1.98.0 test --workspace
cargo +1.98.0 test -p hoimin-cli --test run_e2e
```

Required CI must then pass on Ubuntu, Windows, and macOS using the pinned
toolchain. The canary workflow is validated structurally in this change; its
first scheduled or manually dispatched result is follow-up evidence rather
than a prerequisite for merging the required-CI repair.

Focused mutation testing covers changed production setup code. At minimum, a
counterfactual that executes setup inline instead of through `spawn_blocking`
must be detected by the current-thread heartbeat regression. Workflow and
test-only changes have no Rust production mutation requirement.

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
