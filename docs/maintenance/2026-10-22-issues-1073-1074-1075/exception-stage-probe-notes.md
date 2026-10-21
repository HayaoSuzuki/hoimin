# Temporary exception-project stage measurement

`measure-exception-stages.py` generates a temporary integration probe and inserts callbacks into the actual production loader/index builder. It does not execute Cargo. Run only while the checkout is otherwise idle. Its backup directory records the exact original bytes and patched hashes for three production files; restore rejects changes made after applying the probe. Restore accepts already restored files so an interrupted restoration can be retried. The generated test is removed on restore. Do not format or edit instrumented production files before restoration.

In the final checkout, run these operations sequentially, restoring even if Cargo fails:

```sh
python3 docs/maintenance/2026-10-22-issues-1073-1074-1075/measure-exception-stages.py apply-baseline .
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test exception_stage_probe -- --nocapture
python3 docs/maintenance/2026-10-22-issues-1073-1074-1075/measure-exception-stages.py restore .
python3 docs/maintenance/2026-10-22-issues-1073-1074-1075/measure-exception-stages.py apply .
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test exception_stage_probe -- --nocapture
python3 docs/maintenance/2026-10-22-issues-1073-1074-1075/measure-exception-stages.py restore .
```

`apply-baseline` reads precisely the three relevant production files from `03c7e00` using `git show`; its restore reinstates the current implementation's original bytes. `apply` instruments those current bytes. Both validate every replacement anchor before writing anything. Fixture creation and runtime/parser warmup occur before callback installation. The three projects contain exactly 1/4/16 files of 65,536 bytes each, with a single comment line and fixed Root/Child definitions. Only m00.py is targeted; the probe checks the actual Child-to-Root handler mutation and empty diagnostics.

The printed measurements mean:

- `acquire_peak`, `parse_peak`, `index_peak`: maximum requested live heap above the loader-entry baseline reached during acquisition/decode/copy, per-file parsing/summary extraction, and final import/class/visibility construction. These include allocations retained from earlier stages, making source-retention overlap visible. Each phase begins a fresh allocator window; acquisition ends before parsing starts in the incremental implementation, so windows never nest. Allocations on the actual blocking worker are included by the existing global allocator.
- `summary_retained`: live heap immediately before final index construction, minus the baseline implementation's actual retained source String capacities. Includes summaries, hashes, paths, module reservations, collection metadata, and other loader bookkeeping. It is a retained-summary-and-metadata measure, not an allocation attribution to Module alone.
- `index_retained`: live heap after final index construction and after dropping baseline source bodies. Includes index and remaining loader metadata. Source-drop timing is changed only after the final-index measurement to expose this snapshot; source bodies still overlap every baseline parsing and final-index stage.
- `acquired_decoded_peak`: largest logical decoded text view alive during acquisition. This is a byte-length observation, not a claim that decoding owns another allocation; ASCII decoding can borrow raw input.
- `project_copy_peak`: maximum sum of actual owned source String lengths in the baseline loader's collection. The incremental loader has no such collection.
- `decoded_plus_copy_peak`: maximum simultaneously live logical decoded bytes plus project-owned body-copy bytes, measured before/after each real collection insertion. The decoded view may borrow raw file bytes; do not interpret this sum as requested heap.
- `body_capacity`: baseline source-body String capacities before those Strings are dropped. This is what is subtracted for the summary snapshot; the incremental value is zero.
- `baseline` and `stage_start` are raw observer bookkeeping, not stable performance targets.

The callback uses a function pointer and stack/static counters; it does not allocate. No permanent observation API, heap thresholds, RSS inference, mock loader, or synthetic copy control is introduced. The separate permanent regression test provides the bounded-growth assertion. This probe reports measured stage/requested-heap and ownership facts only.

Both baseline and incremental probes were executed successfully on macOS arm64; exact source bytes were restored after each. The values observed are recorded in the adjacent README.
