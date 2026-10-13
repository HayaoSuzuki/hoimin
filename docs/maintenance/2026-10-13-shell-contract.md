# Shell coordination contract (#844)

Base: latest fetched main, `1713c3a17face763839ecd93793db52de8147f3f`.
Branch: `docs/844-shell-contract`. Before starting, main was fast-forwarded;
the merged #843 worktree and branch were removed after tree equality with main
was checked. Its ignored verification logs were preserved outside that worktree.

## Design

Add a short English module contract at the start of `shell.rs`. Explain the
ownership and acceptance boundaries that maintainers need before changing the
execution loop. Link the existing development guide for deadline details and
identify native tests and model-comparison entry points. Clarify the guide's
borrowed-writer limitation; link the source contract from the guide and existing
runtime OKF concept. Keep implementation, tests, README and model files unchanged.

### Design self-review

1. Traced `transition`, `run_loop_context`, dispatch helpers and completion
   acceptance: core selects state/effects; shell performs I/O and supplies events.
2. Compared process `Arc`/request ownership with `BlockingEffect` and
   `BlockingEffectCompletion`: payload acceptance returns workspace state;
   joining a wrapper alone does not establish a core completion.
3. Checked `establish_shutdown_budget`, `drain_processes` and outer close:
   preserve the first budget; expiry can recover buffered ownership without
   applying successful completion transitions. Blocking work can outlive waits.
4. Read cleanup admission, local attempt state and final report handling:
   `DiskLifecycle` grants safety permission; `CleanupProgress` prevents repeated
   local attempts. Delivery cleanup precedes final output acknowledgement.
5. Compared `ReportDelivery::Inline`/`Owned` and their callers: owned tasks retain
   writer/root leases, while synchronous borrowed writers require progress.
   Model proofs and selected implementation correspondence have distinct scope.

### Plan self-review

1. Limited source changes to a leading `//!` block; preserve the auditable loop
   and avoid helper extraction, trait additions or behavior changes.
2. Chose a guide link plus a short guide correction instead of copying the
   shutdown procedure into the source contract.
3. Selected existing shell/report tests and shutdown/disk oracle files as
   navigation targets; no wording tests or new test framework.
4. Chose formatting, a Rustdoc build with warnings denied, strict docs build,
   source-link/OKF checks and a byte comparison of executable source against
   main. Full Rust tests and Lean proof builds would not validate this prose.
5. Planned serial checks with two Cargo jobs, no debug/incremental artifacts,
   dedicated `target/issue-844`, sampled memory/disk guards, and cleanup after
   verification. Obtain a fresh independent content review before committing.

## Work and verification

- [x] Add source contract and guide/OKF navigation.
- [x] Complete five implementation self-reviews.
- [x] Run the selected documentation checks and five verification self-reviews.
- [ ] Obtain independent review, record results, clean target and commit.

### Implementation self-review

1. Compared the leading contract with dispatch and `CompletionSource` handling.
   Limited workspace-transfer wording to workspace-bearing blocking effects:
   candidate reads also use blocking work but do not transfer a workspace.
2. Followed `drain_processes` through buffered acceptance and wrapper abortion.
   Described recovery/accounting without promising core completion or stopping
   already-running blocking operations; outer close preserves janitor fallback.
3. Checked successful finalization against `CleanupFinished`, report write/flush,
   delivery cleanup and `FinishRequested`/`OutputEmitted`. The ordering statement
   is scoped to the successful path and retains failure evidence.
4. Compared report delivery state and the `Send + 'static` owned entry points
   with generic borrowed APIs. Corrected the guide's unqualified inline-write
   wording without changing deadline formulas or promising synchronous preemption.
5. Reviewed module size, navigation and evidence scope: 44 leading documentation
   lines; no duplicate guide procedure or README expansion. Existing tests and
   model files are named, and model proofs are distinguished from native behavior.

### Verification self-review

1. Compared source bytes with the base revision: the complete prior `shell.rs`
   remains an unchanged suffix after the new `//!` block. Other tracked changes
   are Markdown; no executable Rust, test or model edits.
2. Ran `cargo fmt --all -- --check` and `git diff --check`: passed.
3. Built Rustdoc with `RUSTDOCFLAGS='-D warnings' cargo doc --locked -p hoimin-cli
   --lib --no-deps`: passed, including the three public intra-doc links. This is
   documentation compilation, not a fresh run of runtime or model tests.
4. Ran `.venv/bin/python -m tools.docs_site --output site`: passed. The helper
   performs the strict MkDocs build plus generated internal link/anchor checks;
   the development page was generated. Referenced source/test/model files and
   the shutdown guide heading exist. External URL availability was not checked.
5. Parsed all 48 OKF documents for YAML/reserved-file structure and checked the
   new source hash, unique source IDs, citation and existing index reachability.
   Read the new claims against their sources; historical evidence was retained.

Rustdoc ran alone with two Cargo jobs, debug/incremental output disabled,
`CARGO_TARGET_DIR=target/issue-844`, a 180-second timeout, an 8 GiB sampled RSS
guard, 5 GiB target guard and 10 GiB free-disk floor. It finished in 24.55 seconds;
sampled peak RSS was 590 MiB. The strict site build used the same process-group
RSS/time monitor and produced 3.6 MiB of output. Sampling is not a hard allocation
cap. The new worktree has its own frozen docs-only Python environment.

No full Rust test suite, new wording tests or Lean build ran for this
documentation-only change. The local design assessment is 10/10 for the affected
boundary: the previously missing overview now exposes responsibility/ownership
without adding APIs. This is not a repository-wide architecture score. Prose
review scored directness 9, rhythm 8, trust 9, authenticity 8 and density 9 (43/50).

### Independent review

A fresh independent reviewer (`independent_review_844`, 2026-10-13) inspected the
working diff, maintenance record and relevant implementation without editing or
running builds. No critical, important or minor findings; ready for commit.
The review confirmed buffered ownership recovery versus core acknowledgement,
cleanup/final-output ordering, the owned/borrowed writer distinction, and the
limits of model evidence. Reported build/resource measurements and historical
workflow steps were not independently certified by that content review.
