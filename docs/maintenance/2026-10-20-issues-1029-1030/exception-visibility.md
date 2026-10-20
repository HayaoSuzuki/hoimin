# Exception visibility member indexing (#1029)

Base: `1f7feae33fa310aae87fbadb390105f6d770439d`. Plan: `docs/superpowers/plans/2026-10-20-issues-1029-1030.md`, Task 2.

## Root cause and design reviews

The old loaded-module loop walks every `target.bindings` entry for every import alias. It allocates `alias.member` before finding whether that member denotes an eligible exception class. With N ordinary bindings and A aliases this performs N × A unnecessary scans and resolutions even with zero visible output.

Five design passes, completed before changing the algorithm:

1. **Eligibility:** `binding_id(importer, alias.member)` requires the target binding to be a direct `BindingKind::Class`. Its result must then occur in `self.classes`. Every potentially accepted target member is therefore already a key in `self.classes`; grouping those borrowed names by module path is sufficient. Reexported names remain excluded as qualified members. Direct `from` imports keep their existing path.
2. **Context:** The index stores only member names. Importer-specific `binding_id` still checks dynamic state, alias binding, exact module identity, and loaded modules. No resolved identity is reused across importers.
3. **Order and timing:** Preserve exact/descendant loaded-module traversal and lexical member order. `classes` is a BTreeMap of `(path, name)`, so grouped vectors have the same member ordering as filtered bindings. Preserve the final alias sort and `max(binding.end, loaded_end)` visibility boundary.
4. **Bounds and cancellation:** Borrow names only for the duration of `build_visible`, with at most one name per resolved class. Keep the global summary debit on each emitted local/qualified spelling. Check cancellation while constructing the member index and retain checks in the loaded-module and output loops, including empty modules.
5. **Evidence:** Independent scan and alias-resolution counters vary N and A separately. A class-bearing case checks the unavoidable A × C output. Before/after release measurements isolate fixture setup and distinguish cumulative requested bytes, retained requested heap, peak requested heap, and elapsed time.

Ruling: group names from the existing resolved-class table instead of scanning all module bindings to rediscover eligibility. The `binding_id` branch plus the subsequent membership filter proves this necessary-condition filter; importer resolution remains unchanged. If this proof is invalid, the risk is a missing qualified class spelling, covered by the visibility reference and public hierarchy oracles.


## Implementation reviews

1. **Necessary condition:** Inspected `binding_id` and its unchanged final `classes.contains_key` filter. A successful qualified name necessarily identifies the direct `(target path, member)` class entry used by the new index. The implementation does not admit imported/reexported names merely because the target resolves them locally.
2. **Borrowing and retention:** The temporary map borrows `Utf8Path` and `str` from `classes`; it does not duplicate paths, names or class records. It is dropped on return from `build_visible`. No retained `ExceptionIndex` field or public interface changed.
3. **Ordering and offsets:** Confirmed tuple-key BTreeMap iteration preserves member lexical order within each path. Loaded traversal, importer-specific resolution, alias sorting, and binding/loaded offset maximum remain unchanged. The exhaustive reference compares full visibility maps, including offsets.
4. **Resource handling:** One cancellation poll is added for each indexed class. Existing polls in outer bindings, empty loaded modules and emitted member loops remain. Summary debits occur at the same successful-emission points; exact/excess 65,536-spelling tests pass.
5. **Scope and instrumentation:** Production diff adds one local index and substitutes its member iteration. Counters are `cfg(test)` and thread-local; no manifest, dependency, default operator, import-depth rule or saved schema changed. The ordinary-count and alias-count matrix uses separate generated AAA tests.

## Verification reviews

1. **Meaningful RED:** `visibility-red-complete.log` reports 16 expected failures and 10 passes before the algorithm change. At 512 ordinary bindings and 32 aliases, the old path scans 16,384 members; with two classes it makes 16,448 alias resolutions. The initial matrix was split into individual macro-generated tests before the final RED run.
2. **Operation GREEN and independent enumeration:** `visibility-green-final.log` reports 26 passing tests. All zero-class cases have zero indexed-member scans. The two-class cases resolve 2, 16 and 64 members for 1, 8 and 32 aliases, independently of 32, 128 or 512 ordinary bindings. A 128-case property test compares the entire map with an exhaustive module/binding enumeration that does not use the new member index. It shares the unchanged `binding_id` semantic primitive; it is not an independent proof of that primitive.
3. **Boundaries and cooperative cancellation:** Exact 65,536 visible spellings are accepted and an additional alias is rejected. Cancellation is observed after 16 member-index visits and after 16 empty loaded-module visits. Literal reexport visibility and loaded descendant spelling order checks pass. These are operation-driven gates, not wall-time assertions.
4. **Existing behavior and public boundaries:** `cargo test -p hoimin-python --lib exception_hierarchy` passes 100 tests, including existing import-depth and alias-oracle cases. Public CLI tests pass 22 hierarchy, 5 scope and 2 Lean-oracle tests. The unchanged release probe's 28 public API cases compare full ordered candidates (including stable IDs), diagnostics and truncation at limits 0, 1, 2 and 100. All before/after results match. It covers loaded/unloaded descendants, alias/class shadowing, reexports, ambiguous identities and skipped-site diagnostics.
5. **Measurement review:** Completed seven serial paired samples for each of eight workloads, alternating revision order. Full outputs match for all samples, all 28 API contract cases and all eight CLI fixture/limit pairs. A 30,000-binding/30,000-alias CLI input exceeds the 10 ms deadline on both revisions: exit 2, zero stdout bytes, `analyzer.timeout` on stderr. Fixture generation and output characterization are outside `ExceptionIndex::from_sources_with_inputs` timing and allocation measurement. The same allocator implementation and probe source are linked against both revisions, with source/rlib/binary SHA-256 metadata. The meter checks known allocation/grow/shrink/free requests before each run.


## Release results

Host: macOS 15.7.7, ARM64. Base release: `1f7feae33fa310aae87fbadb390105f6d770439d`. After release contains the combined #1029/#1030 change; only exception hierarchy is enabled in this probe. Raw measurements and all full-output comparisons: [visibility-measurements.json](visibility-measurements.json). Exact linker arguments, compiler version, source hashes and linked rlib hashes: [before](receipts/visibility-before.build.json), [after](receipts/visibility-after.build.json). The [source manifest](production-source-manifest.json) identifies the production-source hashes used for both releases.

Each process performs one warmup build and drops it, then measures one `ExceptionIndex::from_sources_with_inputs` call. Source construction, roots, public API characterization and JSON serialization are outside the measurement. Seven paired samples alternate execution order; all builds/tests were stopped during this interval. Times below are medians; ranges are retained in the JSON. The first small baseline sample had an outlier (175.292 µs versus a 23.041 µs median), so small timing differences are not treated as a guarantee.

N is ordinary target bindings, A import aliases, and C resolved classes. The class-bearing cases retain all A × C qualified spellings; C−1 related candidates are checked through the public API.

| N / A / C | Time, before → after (ms) | Cumulative requested allocation, before → after (bytes) | Retained requested heap (bytes, unchanged) | Peak requested heap, before → after (bytes) |
| --- | ---: | ---: | ---: | ---: |
| 8 / 1 / 0 | 0.0230 → 0.0216 | 23,338 → 22,610 | 5,831 | 7,930 → 7,930 |
| 8 / 1 / 2 | 0.0310 → 0.0296 | 36,014 → 35,806 | 10,789 | 10,848 → 11,309 |
| 1,000 / 1 / 0 | 0.766 → 0.511 | 1,291,442 → 1,200,442 | 184,375 | 607,137 → 607,137 |
| 32 / 1,000 / 0 | 10.036 → 0.659 | 4,406,590 → 1,494,590 | 191,807 | 980,340 → 980,340 |
| 1,000 / 1,000 / 0 | 296.884 → 1.215 | 93,640,894 → 2,640,894 | 366,007 | 1,154,540 → 1,154,540 |
| 2,000 / 2,000 / 0 | 1,224.512 → 2.443 | 369,273,582 → 5,273,582 | 728,271 | 2,305,344 → 2,305,344 |
| 1,000 / 1,000 / 2 | 302.095 → 1.952 | 94,039,192 → 3,039,712 | 516,165 | 1,155,449 → 1,155,449 |
| 64 / 256 / 16 | 7.381 → 2.407 | 2,879,068 → 1,389,028 | 373,563 | 381,745 → 382,457 |

The temporary borrowed-name index increases peak requested heap by 461 bytes in the small class-bearing fixture and 712 bytes in the 4,096-qualified-spelling fixture. Retained heap is unchanged in every case. Cumulative allocation counts successful allocation requests and the full new size of successful reallocations; retained/peak heap track simultaneously outstanding requested sizes relative to the caller-owned setup. These are neither RSS nor allocator usable bytes.

The deterministic counters cover indexed member visits and qualified alias resolutions. Local binding traversal still occurs once per module binding, and other analyzer stages remain part of the measured total. This change removes repeated unrelated target-member scans; it does not promise constant work independent of visible output, hierarchy size or import graph size.

## Reproduction and receipts

Build the base and changed release into separate Cargo target directories, then run from the changed repository root:

```sh
python3 docs/maintenance/2026-10-20-issues-1029-1030/build_probe.py /path/to/base-target docs/maintenance/2026-10-20-issues-1029-1030/visibility_probe.rs /tmp/visibility-before
python3 docs/maintenance/2026-10-20-issues-1029-1030/build_probe.py /path/to/after-target docs/maintenance/2026-10-20-issues-1029-1030/visibility_probe.rs /tmp/visibility-after
python3 docs/maintenance/2026-10-20-issues-1029-1030/visibility_measure.py /tmp/visibility-before /tmp/visibility-after /tmp/visibility-measurements.json --cli-before /path/to/base-target/release/hoimin --cli-after /path/to/after-target/release/hoimin
cargo test -p hoimin-python --lib visibility_tests -- --nocapture --test-threads=1
cargo test -p hoimin-python --lib exception_hierarchy
cargo test -p hoimin-cli --test exception_hierarchy --test exception_hierarchy_scopes --test lean_exception_hierarchy_oracle
```

The link helper fails on ambiguous stale rlib variants. Runnable binaries for this execution were preserved at `/private/tmp/visibility-before` and `/private/tmp/visibility-after`; receipts preserve their SHA-256 values. Fresh reproduction does not depend on those temporary binaries.

- [Expected RED](receipts/visibility-red.log): 16 failures, 10 existing-behavior checks pass.
- [Focused GREEN](receipts/visibility-green.log): 26 pass.
- [Hierarchy/alias/import-depth tests](receipts/visibility-hierarchy.log): 100 pass.
- [Public CLI tests](receipts/visibility-cli.log): 22 + 5 + 2 pass.
- [Final measurement run](receipts/visibility-measure-final.log): successful serial run and persisted JSON.

The first measurement driver rejected the CLI's expected exit 4 for a successful truncated plan. [That harness failure](receipts/visibility-measure-harness-error.log) was diagnosed from its zero-error stderr and the existing plan exit contract. The driver now accepts 0/4 for plan output and checkpoints raw samples before CLI checks. The complete isolated measurement run was repeated after fixing the harness. No production behavior changed for this correction.

This is synthetic macOS evidence. The existing checked-in Lean oracles were exercised; no new formal proof or native Linux/Windows result is claimed. Parent-owned whole-workspace tests and all-feature lint gates are recorded in the combined issue report.
