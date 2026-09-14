# Issue 458 review record

Base: `8b33167`. Worktree: `.worktrees/issue-458`. Reviewer: Codex. No implementation success is claimed by the design review.

## Design self-review

1. Compared the issue acceptance list with both dispatch functions and existing metrics finalizer. Finding: changing only borrowed `run_with_io` would omit real CLI `run_from`; both routes are now explicit in the design.
2. Followed run path conversion and preflight destination permission. Finding: relative metrics paths use invocation cwd, not plan root; shared normalization is required. Baseline failures must preserve shell output authorization.
3. Re-read the revised design against saved plan serialization and public preparation signatures. Output-only attachment preserves existing callers and schema; preparation timing is explicitly excluded from shell metrics.

## Plan self-review

1. Mapped explicit/top and strict/diverse acceptance to regression cases, including multiple candidates and independent IDs.
2. Checked source paths and interfaces in current `cli.rs`, `lib.rs`, `plan.rs`. Both owned and borrowed dispatch routes consume the same parsed path.
3. Checked failure sequencing: preparation failure precedes attachment; collision checks and write warnings remain in shell. Added separate tests rather than assuming parser success proves export.

## OKF work

Consulted `docs/knowledge/design/selection-plan-verify.md` and `docs/okf-workflow.md`; existing historical revisions remain historical. The corresponding selection contract and both source indexes are updated.

## Implementation self-review

1. Compared the extracted normalization helper byte-for-byte with the previous run closure: cwd joining, non-UTF-8 rejection and error labels are preserved. Both owned and borrowed dispatch attach metrics after preparation and before shell preflight.
2. Followed output configuration into destination authorization and finalization. Selected sources and fingerprint inputs retain existing protection; failed preparation cannot write a sidecar. No selection or limit mutation was introduced.
3. Reviewed the final diff against the saved-plan API and schema. Public preparation function signatures remain unchanged; the new `VerifyArgs` field is output-only. Existing result headers legitimately include the operational output configuration, so report parity excludes that field explicitly.

## Test self-review

1. Initial parser regression failed on unexpected `--metrics`. Setup review found the fixture marker inside the project, which deliberately triggers `workspace.original.changed`; moved the marker to an independent directory and used the existing survived exit code 1. This was a fixture correction, not a production workaround.
2. Read every new assertion against the report schema. Replaced a null-versus-null `run.limits` assertion with the real `run.normalized_config.limits` object and compared the remaining normalized configuration after explicitly removing output. A total-timeout case cannot require accepted `baseline_finished`; retained that assertion only for an ordinary baseline failure and required final JSONL output and valid zero-execution metrics for both.
3. Shared Cargo target reuse produced a zero-test run from another worktree. Excluded those runs from evidence and rebuilt in this worktree's dedicated target. Temporarily restored base production code: the new selection regression failed with unexpected `--metrics`. Restored the implementation and reran all five new tests successfully. The real-binary test independently checks relative cwd, help, IDs/verdicts, limits and output-independent configuration.

## OKF self-review

1. Compared the new contract with the final CLI and both dispatch routes. Reused the existing selection concept; retained unrelated historical source metadata and draft status.
2. Traced both new source-index entries, source IDs and footnotes to the actual design and report files. YAML parsing and local-link checks are separate from these implementation claims.
3. Read the Japanese addition for scope and evidence: plan preparation is excluded from metrics stages, failure before shell leaves the destination untouched, and native enforcement is not inferred from macOS. Updated hashes after the final document edits.

## PR self-review

1. Matched every Issue #458 acceptance item to implementation, a regression or the documented preparation-failure rule. Both explicit IDs and strict/diverse multi-candidate batches export the existing sidecar.
2. Checked that staged scope contains only the option, shared normalization, dispatch wiring, tests and documentation. The local `.venv` symlink and build artifacts must not be committed. The PR template must distinguish local checks from pending CI.
3. Reviewed the concrete PR text for result/metrics separation and actual evidence. Do not claim full workspace, wheel or non-macOS testing; no plan schema or execution override is introduced. Publication is checked after creation.

## Verification evidence

Dedicated worktree target, Rust 1.98.0, macOS. Build environment: `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`.

- `cargo test --offline -p hoimin-cli --test plan -- --test-threads=2`: 60 passed, 1 ignored subprocess fixture.
- `cargo test --offline -p hoimin-cli --test plan verify_metrics -- --nocapture`: final sensitivity restoration, 5 passed.
- `cargo test --offline -p hoimin-cli --test run_e2e metrics -- --test-threads=2`: 3 passed, including existing metrics schema and semantic validation.
- `cargo clippy --offline -p hoimin-cli --all-targets --all-features -- -D warnings`: passed.
- OKF validator: 19 Markdown pages, 757 local links, YAML/reserved files, source IDs/footnotes, issue-458 hashes and complete design/report source indexes passed.

No production Python changed; Python mutation testing is not applicable. Full workspace tests, release wheel and native Linux/Windows enforcement were not run locally. The branch reuses existing shell shutdown and destination semantics; it does not add a new timeout or filesystem model.

## Independent review

The issue-471 agent reviewed production dispatch, path normalization and tests. It found no production blocker, but identified that a 100 ms total timeout may expire during preflight on a loaded host. The test now requires a baseline stage only for the ordinary baseline-failure case; timeout still requires a valid sidecar, zero executed mutants and final JSONL output. This avoids treating an unstarted baseline as a required observation. Parent reviewed and applied the finding.

Publication checked through GitHub: PR #537 targets main, contains 10 changed files, and its implementation head is `88c14025a9a27c2fa562ce32ce901afdeed159bc`. Parent confirmed the URL, title, base and head after push. Hosted CI was not treated as passed at publication.
