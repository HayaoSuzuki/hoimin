# Windows Real PID Reuse Feasibility Remainder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve GitHub issue #223 as an evidence-backed Windows platform remainder without weakening or falsely satisfying its real-PID-reuse acceptance criteria.

**Architecture:** Keep the production UUID-plus-owned-handle generation barrier unchanged. Record why the durable old-generation handle and a concurrently reused numeric PID are mutually exclusive under supported Windows APIs, and retain the existing bounded real completion-port tests as non-closing evidence.

**Tech Stack:** Rust 2024/MSRV 1.88, `windows-sys`, Windows Job Objects and I/O completion ports, GitHub Actions `windows-latest`, Markdown documentation.

## Global Constraints

- Capture a real assigned process generation, including a durable process handle, let it exit, and retain its delayed cleanup state.
- Deterministically and boundedly obtain a second real process with the same numeric PID but a different generation; unbounded PID churn and timing loops are forbidden.
- Run cleanup for the old generation through the production Job Object path.
- Prove through real process handles that the new generation remains active and receives no termination request while the old generation is retired.
- Cross the real completion-port path; synthetic PIDs, `record_notification`, and direct `RunState` mutation do not count.
- Bound the complete fixture and cleanup.
- If deterministic real PID reuse is not enforceable in Windows CI, retain #223 as the explicit platform remainder and do not claim closure.

---

### Task 1: Establish the supported Windows boundary

**Files:**
- Inspect: `crates/hoimin-cli/src/resource/suspended.rs`
- Inspect: `crates/hoimin-cli/src/resource/windows.rs`
- Inspect: `.github/workflows/ci.yml`
- Create: `docs/superpowers/reports/2026-08-04-windows-real-pid-reuse-feasibility.md`

**Interfaces:**
- Consumes: `SuspendedChild::open`, `register_root`, `ActiveRoot::drop`, `WindowsRunJob::drain_until_root_exit`, Microsoft Win32 process and Job Object contracts.
- Produces: a line-by-line feasibility matrix and an explicit closure recommendation.

- [x] **Step 1: Trace the production generation lifetime**

Record the ownership sequence from `OpenProcess` through both Job Object
assignments, `ActiveRoot::process`, completion dequeue, and handle close.

- [x] **Step 2: Verify PID and completion-port contracts from primary sources**

Use the Microsoft `PROCESS_INFORMATION`, `CreateProcessW`,
`JOBOBJECT_ASSOCIATE_COMPLETION_PORT`, and Job Objects documentation linked in
the report. Confirm that PID reuse starts only after every old process handle is
closed and that process creation has no requested-PID input.

- [x] **Step 3: Run a bounded corroborating probe**

Retain one exited real process handle, verify its PID and exit code through the
handle, create exactly 64 additional real processes, and enforce a 30-second
outer bound. Report the literal observation without treating a finite sample as
proof.

- [x] **Step 4: Write the feasibility report**

Map each acceptance criterion to reachable or unreachable production behavior,
include failed diagnostic attempts, and state that existing real completion-
port tests are bounded but do not cover PID reuse.

### Task 2: Specify the non-implementation decision

**Files:**
- Create: `docs/superpowers/specs/2026-08-04-windows-real-pid-reuse-feasibility-design.md`
- Create: `docs/superpowers/plans/2026-08-04-windows-real-pid-reuse-feasibility.md`

**Interfaces:**
- Consumes: the Task 1 feasibility report and issue #223's exact acceptance criteria.
- Produces: the selected platform-remainder design and this executable verification plan.

- [x] **Step 1: Compare three approaches**

Compare the supported handle barrier, close-and-churn, and synthetic or kernel-
controlled allocation. Select the handle barrier and reject any option that
weakens production or fakes the acceptance boundary.

- [x] **Step 2: Define the reopening condition**

Require either supported deterministic requested-PID control in the CI
environment while retaining the retired object handle, or an explicit owner
revision of the acceptance boundary. Do not infer either change from future
allocator behavior.

- [x] **Step 3: Self-review the documents**

Scan for placeholders, contradictions, accidental closure claims, ambiguous
uses of "bounded," and any statement that treats synthetic coverage as real
Windows evidence.

### Task 3: Verify the disposition without changing Rust

**Files:**
- Verify: `docs/superpowers/specs/2026-08-04-windows-real-pid-reuse-feasibility-design.md`
- Verify: `docs/superpowers/reports/2026-08-04-windows-real-pid-reuse-feasibility.md`
- Verify: `docs/superpowers/plans/2026-08-04-windows-real-pid-reuse-feasibility.md`

**Interfaces:**
- Consumes: the documentation-only diff and existing Windows resource tests.
- Produces: fresh format, lint, test, and repository-state evidence; no mutation result because no Rust changed.

- [x] **Step 1: Validate documentation structure and links**

Run:

```powershell
rg -n "T[B]D|T[O]DO|implement[ ]later|fill[ ]in details" docs/superpowers/specs/2026-08-04-windows-real-pid-reuse-feasibility-design.md docs/superpowers/reports/2026-08-04-windows-real-pid-reuse-feasibility.md docs/superpowers/plans/2026-08-04-windows-real-pid-reuse-feasibility.md
git diff --check
```

Expected: `rg` returns no matches and `git diff --check` returns success.

- [x] **Step 2: Run the reachable Windows production-path tests**

Run:

```powershell
cargo test --offline -p hoimin-cli resource::windows -- --nocapture
```

Expected: all 12 Windows resource tests pass, including the two real-process
handle-retention and real completion-port tests.

- [x] **Step 3: Run workspace formatting and strict lint**

Run:

```powershell
cargo fmt --all -- --check
cargo clippy --offline --workspace --all-targets --all-features -- -D warnings
```

Expected: both commands exit successfully with no formatting or lint failures.

- [x] **Step 4: Run the workspace test suite**

Run:

```powershell
cargo test --offline --workspace
```

Expected: all workspace tests pass. If the host lacks Windows link-creation
privilege, record the exact `ERROR_PRIVILEGE_NOT_HELD` fixture failure, reproduce
it in isolation, and rerun the workspace with only that exact test skipped. The
remainder must pass; do not describe the privileged fixture as green.

- [x] **Step 5: Record mutation-testing non-applicability**

Confirm with `git diff --name-only` that no Rust production or test file changed.
Do not run `cargo mutants` against an empty Rust diff; record mutation testing as
not applicable rather than as passing evidence.

- [x] **Step 6: Commit the platform-remainder evidence**

Run:

```powershell
git add docs/superpowers/specs/2026-08-04-windows-real-pid-reuse-feasibility-design.md docs/superpowers/reports/2026-08-04-windows-real-pid-reuse-feasibility.md docs/superpowers/plans/2026-08-04-windows-real-pid-reuse-feasibility.md
git commit -m "docs: record Windows PID reuse feasibility remainder"
```

Expected: one documentation-only commit on
`test/issue-223-real-pid-reuse`, with #223 still open and no push or PR.

### Task 4: Conditional future implementation

**Files:**
- Modify only after the reopening condition is met: `crates/hoimin-cli/src/resource/windows.rs`
- Test only after the reopening condition is met: a Windows-only real-process test adjacent to the production Job Object implementation.

**Interfaces:**
- Consumes: a supported deterministic real-PID allocation mechanism and the unchanged issue acceptance criteria.
- Produces: a red-green regression that crosses production assignment, completion dequeue, generation cleanup, and real-handle liveness checks.

- [ ] **Step 1: Demonstrate allocator control before writing a regression test**

Publish the supported API or controlled-runner contract and a bounded standalone
reproducer that obtains the requested same PID without churn, timing retries,
hooks, injected packets, or private-state mutation.

- [ ] **Step 2: Re-run brainstorming and write a new strict-TDD plan**

Name the production mutation the test catches, write the real-process test
first, observe the expected failure, implement only the necessary production
change, and verify red-green behavior. Until Step 1 is possible, Task 4 remains
intentionally unexecuted and #223 remains open.
