# Task 1 (#1003) execution report

Status: implementation complete and committed; ready for the controller's independent review.
Base: ccef1941710e20cff8d391283cc21bcfbd7650aa.
Implementation commit: 451a4fdd8f6d91007738798a4f17ead601254b05 (`fix(plan): read sources through shared safe capability`).
This report remains at the required ignored SDD path per the controller's ruling; final integration will archive reports/ledger into durable evidence. No report-only commit was created.

## Requirements/design review (before implementation)

1. Path authority: both direct `tokio::fs::read(root.join(...))` consumers bypass PortableFileReader. Expose its existing hash operation narrowly; retain portable-path checks and WorkerRoot capabilities. No alternate opener.
2. Descriptor race: WorkerRoot already uses no-follow capability-relative Unix O_NONBLOCK plus metadata on the opened descriptor. Reuse this implementation, including its Windows boundary. Consumer regressions will synchronize after real TargetHandler resolution and before acquisition, covering FIFO and symlink replacements.
3. Data retention: existing WorkerRoot::hash uses blake3::Hasher::update_reader, whose buffer size is independent of file length. Source records retain hashes only; descriptor validation retains bytes/context for one file at a time. Use a single root reader per consumer operation.
4. Diagnostic ordering: descriptor validation intentionally defers per-file results to requested-ID order. Open reader lazily at the first selected, known candidate so empty/unknown/unselected cases do not acquire a root or hide their errors. Preserve source record sorting and caller-specific PlanError variants while sharing acquisition error mapping.
5. Platform/scheduling scope: run root opening and file operations via spawn_blocking. Started blocking tasks may outlive a cancelled future; no global hard deadline or forced I/O cancellation claim. Native macOS evidence does not prove native Linux/Windows behavior.

Design diagnostic score: current 7.5/10 (6/8 rows). Shared policy leaks into the two direct reads, and consumer interfaces lack explicit acquisition guarantees. Proposed 10/10 (8/8): a narrow plan source-reader boundary hides blocking scheduling and acquisition errors; PortableFileReader continues to own path/open policy. Record design checks again after implementation.

The controller supplies the sequential plan and independent review; no subagents are used. Task-specific focused verification is binding; final integration owns the full suite and combined OKF catalog metadata.

## Implementation and interfaces

- `plan/source.rs` adds the private `SourceReader` acquisition boundary. Its narrow `read`/`hash` operations open one lazy `PortableFileReader` root, retain that capability in an `Arc`, and run root opening and file operations in `tokio::task::spawn_blocking` with the current tracing span. One shared error mapper preserves path context, maps task-join failures explicitly, and delegates portable-path/open policy to the existing reader.
- `workspace/reader.rs` makes its existing safe `hash` operation `pub(crate)`. It still calls `WorkerRoot::hash`, which uses `with_read_file` and `blake3::Hasher::update_reader`; no hashing/open fallback or additional buffer implementation was introduced.
- `plan.rs` routes source-record hashes and requested-candidate full-byte reads through the boundary. Source record order/dedup behavior and per-file candidate-context ownership remain unchanged. No schema, ID, fingerprint, exit-policy or formal-proof changes.
- `plan/validation_tests.rs` covers six post-resolution swaps (hash/validation × FIFO/regular-file symlink/FIFO symlink), sorted normal hashes and duplicate paths, stale bytes after hashing, and root-capability reuse across hash/read. Existing diagnostic-precedence expectations use the shared missing-file message.
- `tests/plan_source_heap.rs` measures the actual public `plan::create` source stage via its existing tracing stage events. The measurement ends synchronously before analyzer discovery starts; successful manifests independently check expected raw-byte BLAKE3 hashes. No production test API was exposed.
- `docs/usage.md` and `docs/knowledge/design/selection-plan-verify.md` describe the source boundary and scoped evidence. Historical provenance is unchanged. Three new source entries record actual working-tree states and SHA-256 values; final integration owns combined catalog maintenance.

### Controller ruling

The shared missing-file wording is accepted: `root-relative file was not found` replaces incidental native OS error text. `PlanError` category, source path and requested-ID precedence must remain intact. The controller explicitly accepted this ruling during the task. No claim of identical native I/O error semantics is made.

### Five implementation review passes

1. All target reads: searched every `read` call in `plan.rs`; the only remaining unrelated read is the manifest artifact reader. Both project-source consumers now call SourceReader. The private module reaches only PortableFileReader; it contains no joined-path file opener.
2. No-follow/regular-file semantics: traced read/hash through PortableFileReader validation, WorkerRoot parent capability, no-follow Unix O_NONBLOCK open and metadata on that descriptor. No alternate unsafe path was found. Native Unix consumer swaps and existing root FIFO/parent-replacement tests pass; Windows still delegates to its existing capability backend without a new native-success claim.
3. Error priority: SourceReader construction only owns a path; first acquisition follows known-candidate and selected-target checks. Existing requested-ID result removal/order is unchanged. Initial verification exposed two tests tied to OS missing-file text, not a priority regression; the approved shared diagnostic expectations now pass the full cross-product precedence tests.
4. Allocation ownership: source hashes never request a complete byte Vec. Root state is reused; per-task ownership is an Arc plus a cloned relative path. Candidate bytes/context are released at the existing per-file scope before the next read. The capability-replacement regression confirms subsequent byte reads use the original opened root.
5. Async/cancel boundaries: root opening and both filesystem consumers execute in spawn_blocking, retaining only owned data. A cancelled caller may detach already-started blocking I/O; its task keeps the root and result resources until completion. The source-reader comment and guidance state this limit. No arbitrary filesystem deadline is claimed.

Final software-design diagnostic score: 10/10 (8/8 rows). Modules each have one stated responsibility; reader interfaces hide platform/open policy and SourceReader hides asynchronous ownership/error mapping; comments specify the lazy-root/cancellation contract; safety policy can change behind PortableFileReader without editing the two consumers; design review and focused boundary tests cover the refactor. No failed diagnostic rows remain within this task's scope.

## Commands and results

All explicit task Cargo commands ran sequentially from the isolated worktree, with `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`. Process/filesystem suites use `--test-threads=2`; the isolated allocator binary uses one test thread. Native tests were run with sandbox escalation as required. Logs are task-owned under `/tmp/hoimin-priority-six-task1-*`.

### RED and sensitivity

1. `cargo test -p hoimin-cli --lib resolved_symlink_swap -- --nocapture --test-threads=2` — **expected RED**, 2 failures: both original consumers followed a replacement symlink to an ordinary source and returned success where rejection was required. Log: `/tmp/hoimin-priority-six-task1-red-symlink.log`.
2. `cargo test -p hoimin-cli --lib resolved_fifo_swap -- --nocapture --test-threads=2` — **expected RED**, 2 failures: each child stopped at its writerless FIFO read until the 10-second watchdog killed it; children were reaped. Log: `/tmp/hoimin-priority-six-task1-red-fifo.log`.
3. `cargo test -p hoimin-cli --test plan_source_heap -- --nocapture --test-threads=1` — **expected RED**, 1 MiB source-stage additional requested-live-heap peak **1,050,196 bytes**, above the independent **262,144 byte** gate. Log: `/tmp/hoimin-priority-six-task1-red-heap.log`.
4. After adding symlink-to-FIFO cases, temporarily restored both original direct reads and ran `cargo test -p hoimin-cli --lib resolved_symlink_fifo_swap -- --nocapture --test-threads=2` — **expected mutation RED**, 2 failures at the watchdog, with both children killed/reaped. A shell EXIT trap restored the exact safe source file, and the final full plan unit suite passed after restoration. Log: `/tmp/hoimin-priority-six-task1-mutant-symlink-fifo.log`.

The swaps use actual `TargetHandler::resolve`; each child prints a readiness token after resolution and waits on stdin. The parent waits for readiness, replaces the file, then sends the resume token. No sleep determines correctness. The timeout is solely a failure/reaping guard.

### GREEN and checks

- `cargo test -p hoimin-cli --lib plan:: -- --test-threads=2` — final **97 passed, 3 ignored, 0 failed** (including all six final swap cases). Log: `/tmp/hoimin-priority-six-task1-final-plan-unit.log`. Earlier intermediate run: 92 passed, 3 ignored, 2 failures in the OS-message expectations; after the approved diagnostic update: 95 passed, 3 ignored. Both intermediate outcomes were inspected.
- `cargo test -p hoimin-cli --lib workspace::root::tests:: -- --test-threads=2` — **30 passed, 0 failed**; includes FIFO rejection and retained-parent read/hash safety. Log: `/tmp/hoimin-priority-six-task1-green-root.log`.
- `PATH=/tmp/hoimin-priority-six-venv/bin:$PATH cargo test -p hoimin-cli --test plan --test fingerprint_inputs --test fingerprint_input_heap --test source_encoding --test missing_source -- --test-threads=2` — fingerprint inputs **30 passed**, auxiliary allocation **1 passed**; stopped at three `missing_source` fixture failures because those tests require worktree `.venv/bin/python`, regardless of PATH. Log: `/tmp/hoimin-priority-six-task1-green-integration.log`. This was an environment setup failure, not omitted evidence.
- Added an ignored worktree `.venv` symlink to the controller-provisioned `/tmp/hoimin-priority-six-venv`. Reran only failed and previously unexecuted suites: `PATH=/tmp/hoimin-priority-six-venv/bin:$PATH cargo test -p hoimin-cli --test plan --test source_encoding --test missing_source -- --test-threads=2` — **plan/verify 114 passed, 1 ignored; source encoding 6 passed; missing-source 3 passed; 0 failed**. Log: `/tmp/hoimin-priority-six-task1-green-public.log`.
- `cargo clippy -p hoimin-cli --lib --test plan_source_heap -- -D warnings` — **passed**, no warnings. Log: `/tmp/hoimin-priority-six-task1-clippy.log`.
- `cargo fmt --all -- --check` and `git diff --check` — **passed**.
- `/tmp/hoimin-priority-six-venv/bin/python` with PyYAML `safe_load` — parsed the changed OKF concept, checked mapping/nonblank type, all three added source targets/SHA-256 values/footnote definitions, and absence of undefined footnotes: **passed**. This is focused structure/provenance validation; it does not refresh historical evidence or validate the complete combined bundle.

### Heap measurements

The first safe run measured 1/16/64 MiB source peaks of **1,971 / 1,248 / 1,248 bytes**, all below 256 KiB, while producing the expected source hashes. Log: `/tmp/hoimin-priority-six-task1-green-heap.log`. Final command: `cargo test -p hoimin-cli --test plan_source_heap -- --nocapture --test-threads=1` — **1 passed, 0 failed**, with the same **1,971 / 1,248 / 1,248 byte** peaks after exact mutation restoration. Final log: `/tmp/hoimin-priority-six-task1-final-heap.log`. Measurement is allocator-requested extra live heap during source hashing, not process RSS, allocator overhead, stack, analyzer memory, total runtime or all filesystem blocking behavior.

## Five validation review passes

1. Test sensitivity: ordinary symlink and FIFO cases failed against the original implementation; whole-source allocation failed the new gate before implementation. Added FIFO-symlink cases also fail against restored original consumers. Final GREEN follows exact source restoration.
2. One-case AAA: six generated tests each name one consumer/replacement contract. Fixtures are created before act; only the synchronized replacement occurs between resolution and acquisition. Normal fingerprint output, stale-byte validation and root reuse each have a separate behavior test. Existing broader descriptor matrices retain their independent fixture expectations.
3. Boundary completeness: tests exercise the real target resolver, public plan creation, existing regular-file reader/root boundary, candidate raw-byte hash/span/ID checks and saved-plan baseline rejection. Requested-ID priority cross-products and encoding integration passed. The normal hash expectation uses raw fixture bytes rather than the source-record implementation.
4. Platform limits: executed on native macOS only. Unix FIFO/symlink race tests are cfg(unix). The heap fixture supplies platform-correct argv variants but is not claimed to have run on native Linux or Windows. Existing pure Windows disposition/name tests are not native Windows I/O evidence.
5. Resource use: one Cargo operation at a time, 2 build jobs, bounded test threads; watchdog children reaped on each expected RED. One allocator test owns the entire process measurement window, with fixture/expected-hash setup outside it and analyzer work after it. The target occupied about **3.8 GiB**, with **109 GiB** available on the volume when checked; no unrelated files/targets were deleted. Full integration and combined catalogs remain the controller's final batch gate per the binding brief.

## Risks and handoff

No unresolved implementation defect was found in these self-reviews. Pending independent controller review is required before the next task. Filesystem calls on the blocking pool can still outlive caller cancellation; this change is a regular-file acquisition/race and heap fix, not a universal deadline guarantee. Error text for missing sources intentionally comes from the shared reader. The controller's changes in `docs/superpowers/plans/2026-10-19-priority-six.md` are excluded from Task 1 commits.


## Commit verification and controller handoff

Implementation commit: **451a4fdd8f6d91007738798a4f17ead601254b05**.
The Git commit's repository-mandated prek hooks also passed:

- `cargo fmt --all -- --check`
- `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`

These were invoked automatically by the commit hook after explicit verification;
the git command did not explicitly set task Cargo target/job variables. The hook
finished successfully before handoff; no concurrent task Cargo operation ran.
Prek temporarily saved and restored the controller's unstaged plan update. Final
`git status --short` contains only that controller-owned plan document; the Task 1
product changes are committed. The report itself is intentionally ignored and
remains available at its prescribed path. Independent review and the full batch
integration gate remain the controller's responsibility.
