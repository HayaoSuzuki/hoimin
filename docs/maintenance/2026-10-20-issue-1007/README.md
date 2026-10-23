# Immutable workspace manifest evidence (#1007)

The public `WorkspaceManifest` value owns one private immutable shared storage. This removes deep metadata copies while keeping worker filesystem ownership, reservation, copy charge and cleanup independent.

Measurements compare preserved original release `34e3349099ce7fd61838629ee8c0d0b814e65a5d` with the #1007 implementation on Task1 base `4c906846e4515735d7e89c17a2b374483ba299ff`. Both probes link release rlibs with identical `rustc -O -C lto=thin -C embed-bitcode=yes` flags. `verification.json` identifies sources, tools, rlibs and binaries. The original release directory was read only.

Each of 24 workloads has three alternating before/after pairs (144 independent processes). The complete fixed grid is 1000/4000 files, 1/4/16 owners, 0/64 bytes per file, and clone/actual-worker mode. Overall measurement elapsed was 368.76s. Each subprocess has a120s timeout; only one runs at once, and workers are deleted sequentially before the next invocation.

Setup/preflight/grants/requests/output Vec capacities are excluded. Retained requested bytes are read while all handles remain live; peak requested bytes are separate. A meter self-check validates live allocation, realloc growth/shrink, release and peak before each workload. Full content reads, digest/equality and charge assertions, printing and cleanup run outside the measured windows. Every pair agrees on manifest digest, logical bytes, worker charge and logical snapshot-plus-worker payload. There are no RSS measurements or physical disk-block measurements. Reported payload bytes describe logical file data; they do not include filesystem allocation or directory metadata.

| Files | Owners | Clone retained before/after (bytes) | Actual worker retained before/after (bytes) |
| ---: | ---: | ---: | ---: |
| 1000 | 1 | 118,400 / 0 | 118,668 / 268 |
| 1000 | 4 | 473,600 / 0 | 474,312 / 712 |
| 1000 | 16 | 1,894,400 / 0 | 1,897,096 / 2,696 |
| 4000 | 1 | 469,400 / 0 | 469,668 / 268 |
| 4000 | 4 | 1,877,600 / 0 | 1,878,312 / 712 |
| 4000 | 16 | 7,510,400 / 0 | 7,513,096 / 2,696 |

These retained values were identical for both payload sizes and all three samples. The after-worker peaks were65,983/66,427/68,411 bytes at1/4/16 workers, respectively, independent of file count and tested payload size. Clone peaks after sharing were0 requested bytes. One-time manifest construction, including its shared control allocation, is outside these incremental windows.

The deterministic CI gates are `workspace_manifest_heap` (4000 empty files,16 clones,16KiB ceiling) and `workspace_worker_metadata_heap` (1000 empty files,4 actual workers,256KiB+8KiB/worker ceiling). Candidate ceilings were drafted from the design, then confirmed against actual RED measurements and locked before product edits:7,510,400 and474,312 retained bytes, respectively. Both failed due to the measured old behavior; both pass after sharing. Each binary has one test to isolate its allocator. The performance registry runs them as retained-heap gates, without time assertions.

Timing is fixed-workload evidence only. Clone timing windows after sharing can fall below the clock resolution and return0ns; this does not mean zero-cost cloning. Filesystem work still scales with files times workers, and actual materialization timings vary in both directions. The table gives medians and ranges for16 owners; all owner counts and all raw observations are in `measurement.json`.

| Files | Bytes/file | Mode | Before ms median (min–max) | After ms median (min–max) |
| ---: | ---: | --- | ---: | ---: |
| 1000 | 0 | clone | 0.257958 (0.235791–0.258041) | 0.000083 (0.000042–0.000958) |
| 1000 | 0 | worker | 1711.187417 (1672.644417–1833.198542) | 1735.129917 (1673.861917–1798.045792) |
| 1000 | 64 | clone | 0.281917 (0.248125–0.309416) | 0.000167 (0.000000–0.000208) |
| 1000 | 64 | worker | 1957.868750 (1914.568041–1981.664666) | 1963.355250 (1941.655541–1964.959958) |
| 4000 | 0 | clone | 1.166209 (1.087292–1.187125) | 0.000042 (0.000000–0.000291) |
| 4000 | 0 | worker | 6973.261333 (6642.805417–7804.501709) | 6688.566000 (6666.620500–7199.263291) |
| 4000 | 64 | clone | 1.162791 (1.114125–1.189333) | 0.000084 (0.000042–0.000208) |
| 4000 | 64 | worker | 8195.393458 (7721.293208–8276.319750) | 7866.152667 (7732.422333–8181.120500) |

For example, the1000-empty-file/16-worker median increased1711.187→1735.130ms while4000-empty-file/16-worker decreased6973.261→6688.566ms. No general materialization speedup is claimed. The4000-file/64-byte/16-worker case retained the same4,096,000-byte worker charge on both revisions, then released it after sequential worker destruction; logical snapshot-plus-worker payload was4,352,000 bytes.

Reproduce with the exact bounded environment and commands in `verification.json`; `build_probe.py` links this same `probe.rs` to each immutable/current rlib directory, and `measure.py` runs the fixed grid. Full lifecycle/TDD/check receipts are recorded in the sibling ignored SDD Task2 report and `/tmp/hoimin-perf-three-task2`. Local macOS verification does not establish native Windows lock/cleanup cases; those remain for final CI.

## Maintained test support

The probe in this directory is the historical measurement source referenced by
`verification.json` and `build_probe.py`. Ongoing allocation regression tests use
[the maintained test helper](../../../crates/hoimin-cli/tests/support/workspace_metadata_probe.rs).
Changes to current test support do not rewrite this archived probe or its receipts.
