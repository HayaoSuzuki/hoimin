# Task 3 research pointers (not proof evidence)

Pinned official Creusot v0.14.0 sources are available in /tmp/hoimin-issue999-design-mnk8miii/upstream/; filenames replace slash with double underscore. No proof has been run by the controller.

- creusot-std/src/std/time.rs: Duration View is Int nanos with bound 0..(u64MAX*1e9+999999999). DeepModel equals View. It specifies new, from_secs, is_zero, as_nanos, checked_add and checked_mul. checked_* bounds are expressed via nanos_to_secs, not direct Dmax; lemmas may be useful.
- Duration::MAX and u64::try_from(u128) did not have explicit extern specs in inspected snippets. This is not a demonstrated failure. If needed use Duration::new(u64::MAX,999999999) and casts with proved ranges in real production implementation, not local unproved extern specs.
- creusot-std/src/invariant.rs shows `impl Invariant for T { #[logic(open)] fn invariant(self)->bool { pearlite! {...} } }`; function boundaries get type invariants, logical function boundaries do not automatically get them. Structural field invariants are automatic. Do not give public TopBudgetProjection a constructor-provenance invariant.
- creusot-std/src/std/num.rs contains NonZero View/DeepModel/invariant and new/get specs; usize is_multiple_of has a spec.
- creusot-std macros support ensures on constants; constant definitions otherwise inline unless private fields block translation. Read pinned source before selecting syntax.
- creusot::trusted_code defaults allow upstream; deny it in owned verified crate and audit local assume/extern_spec/trusted attributes. Official std trusted specs remain explicit trusted boundary.
- Actual generated named goals/contracts must inform Task4 obligation manifest; do not replace semantic scope with a mere goal-count guard.
- Local Linux ARM64 4CPU/8GB engine facts are in environment-handoff.md. CI acceptance is Linux x86_64; do not label emulator times as native CI measurement.

## Fresh generation and hard resource caps

- Added official cargo-creusot main/why3find_wrapper and Why3find command/runner/wenv/calibration source files under the same upstream directory.
- cargo-creusot supports --no-cache and --replay directly, passes -j to Why3find, uses --no-autodetect-provers, and propagates failed prove exit. Empty verification directory can still reach a successful why3find invocation: explicit scope/nonempty checks remain necessary.
- CARGO_TARGET_DIR is respected when already set; cargo-creusot defaults to workspace target/creusot only when unset. It disables Cargo incremental compilation, but normal Cargo freshness can still mean no source regeneration; runner must deliberately ensure fresh verification output.
- Why3find time is reference-machine-relative; final search round uses twice reference time and calibration scales it. runner.ml enforces wallclock timeout*1.5, not exact user30seconds. Need a hard per-process limit independent of calibration (Task2 owner notified; Task4 enforce actual process caps). Why3find memory comes from Why3 config via memlimit.
- Why3find --no-cache bypasses cache reads but still updates cache; isolate/reset owned cache and never use its successful answers as proof. Saved proof.json is a strategy only.

## Dependency/license boundary to investigate in Task 3

Official v0.14.0 manifests declare `creusot-std` and `creusot-std-proc` LGPL-2.1-or-later. creusot-std default features include std + creusot-deps; proc has optional uuid/pearlite-syn under creusot-deps. Source: https://github.com/creusot-rs/creusot/blob/v0.14.0/creusot-std/Cargo.toml and creusot-std-proc/Cargo.toml. This is a declaration fact, not a legal-compliance conclusion.

Investigate keeping annotation imports and creusot-std dependency in cfg(creusot) only, while keeping all production types/bodies/branches identical. Conditional proof attributes/ghost code are permitted; alternate runtime implementation is not. Confirm actual cargo-creusot selection before choosing this boundary. Ordinary stable/all-features builds should not need proof tooling; avoid unnecessary shipped LGPL/runtime dependencies. Do not infer a successful cargo-deny run fulfills license obligations.

Existing policy in docs/dependency-policy.md says a narrow exception must record crate/version, upstream evidence, reason and re-review condition; do not broaden global licenses. docs/licensing.md and licenses/ contain the precedent for optional contracts macros' notice/source/generated-code review. Exact cargo-deny version in CI is0.20.2; root/fuzz allfeatures/no-target-filter actual audit still pending. Build-script inventory and notices/source changes must be handled with final integration.

## Additional pinned Duration regression example

Official v0.14.0 `tests/should_succeed/duration.rs` and its `duration/proof.json` were downloaded into upstream/ (same slash-to-double-underscore convention). The example actually uses Duration::new(u64::MAX, 999_999_999), checked_* and @ nanos assertions. Saved upstream strategy is predominantly CVC5; this is syntax/reference evidence only, not proof of hoimin. Source: https://github.com/creusot-rs/creusot/blob/v0.14.0/tests/should_succeed/duration.rs

## Local host execution ruling

Task2 switched development setup from slow x86_64 emulation on this ARM64 host to native Linux ARM64 (same source pins and proof targets; official host component hashes). All three target generations/proofs remain required. Linux x86_64 native cold/warm CI including setup within30min remains an explicit Task4 acceptance gate. ARM64 local success is not that gate. See ledger for recorded ruling.

Task2 final effective solver commands are to enforce GNU timeout30s plus prlimit(RLIMIT_AS=2GiB), in addition to Why3 memlimit2048 and max2. Use the actual final config/preflight, not an unbounded alternative CLI. This AS cap is stricter than RSS; if a solver needs too much virtual memory, diagnose and retain the cap or an equally strict alternative rather than silently increasing it.

## Final Task2 working environment handoff

Task2 implementation commit336be05e62a3ec6cf47e4949d7e195804d79c177; independent scoped review pending at time of note. Actual image hoimin-issue999-creusot-arm64:local exists and passes11-tool strict preflight. Actual Python3.12.3 inside. Full commands, architecture/disk/resource evidence and3-target ordinary add_one proof/first+fresh-warm results are in .superpowers/sdd/2026-10-19-issue-999-creusot-adoption/task-2-report.md and committed formal/creusot/README.md, smoke.sh. Do not rebuild the image unless a concrete environment change requires it. All final tool-source layers are cached.

A host /tmp bind mount failed because the Docker daemon could not see that path; Task2 used streamed shell over stdin and task-owned ephemeral containers. It did not test a real worktree bind under /Users. For actual source transport, confirm the mount or use docker cp/streamed tracked workspace sources, and prove the current product crate with unchanged workspace manifest semantics. Do not accidentally include host .venv, shared targets, Git metadata or unrelated worktrees. Ensure copied current source is fresh for each sensitivity experiment. Avoid rewriting a verification-only implementation/manifest alternative.

Unchanged warm Cargo can skip Coma regeneration; Task2 observed and documented this, then tested source-mtime-triggered fresh regeneration on its disposable sample. Production runner must establish fresh generation without relying on touching product source. Exact generation/prove command syntax and paths are in final Task2 report.

## Conditional annotation dependency evidence

Pinned source creusot-rustc/src/main.rs and creusot-args/src/lib.rs are now cached in upstream/ (slash-to-double-underscore names). main.rs69–85 adds --cfg=creusot even on its no-contract path (comment says this includes rustc -vV), while contract-bearing compilation gets CREUSOT_RUSTC_ARGS including --cfg creusot. This makes cfg(creusot)-only Cargo dependency activation plausible through Cargo rustc --print cfg, but actual metadata/feature resolution still needs a real check. Do not present this source inference as demonstrated success. The strict environment is ready for that check after Task2 fix review.

## Overflow-obligation source check

CREUSOT_RUSTC_ARGS includes -Coverflow-checks=off, so the controller checked the actual translation instead of assuming compiler debug assertions prove overflow. Cached creusot/src/backend/program.rs maps integer Add/AddUnchecked, Sub/SubUnchecked, Mul/MulUnchecked to program prelude add/sub/mul; cached prelude-generator/int.in.coma requires in-bounds/no-overflow before these program operations. Therefore inspect actual generated VCs for those bounds; do not treat the rustc flag itself as proof coverage. Official prelude specs are the approved trust boundary, not owned trusted holes.
