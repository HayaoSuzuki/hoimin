# Resource cleanup correspondence worksheet

Claim: reserving cleanup ownership before an unlocked OS wait permits unrelated registry operations without exposing deleted counters or allowing duplicate cleanup. Closing admission must precede releasing the registry lock.

Declared behavior is bounded cleanup, retryable failure, and safe workspace retention. The implicit behavior being repaired is global lock ownership throughout the wait. Physical removal and registration removal are separate events: merging them would hide the counter-read race.

| Premise / observation | Lean representation | Production configuration | Public observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Root A cleanup reservation | `beginCleanup`, `cleaning` | Per-root mutex, inactive entry | Not public | Linux cleanup method | model-only |
| Physical removal before map commit | `removeDirectory`, `commit` | OS deletion then map update | Not public | Narrow Linux unit-test callback | model-only |
| Root B registry progress | `probeRegistry`, `registryHeld` | Independent worker | Scheduling not deterministic publicly | `try_lock` in owned test seam | model-only |
| Counter eligibility | `probeCounters`, `active`, `directoryExists` | Counter files and root entry | Resource errors indirectly visible | Fake directories in Linux tests | model-only |
| Close blocks future admission | `close`, `admit` | Closed backend rejects prepare/attach | Backend error | Linux unit tests | model-only |
| Duplicate reservation | Second `beginCleanup` rejected | Root mutex serializes retry | Not public | Root/close gate tests | model-only |

These are deliberate Boolean abstractions, not corpus replay or same-premise implementation comparisons. The Rust tests are independent internal fixtures, not asserted matches against generated Lean observations. No kernel, mutex fairness, memory-ordering, Windows notification ordering, or Tokio runtime shutdown theorem is claimed. Error precedence remains covered by Rust regressions and is outside this model.

The finite alphabet has eight events and one explicit root; all unrelated roots are represented by the registry probe. Exploration starts at depth 2, increases to 3 and then 4 only after measuring cost. Broken variants retain the global mutex, leave deleted counters eligible, admit after close, or allow duplicate cleanup. Their shortest witnesses must be retained. Evaluation is behind a non-imported audit entry point; imported semantics contain only cheap definitions and a reservation lemma. Each local command has a 20-second deadline and 2 GiB RSS cap; CI retains its existing 30-second cap. Results and exact commands are recorded after execution below.

## Results

| Depth | Traces (including prefixes) | Evaluated transitions | Elapsed | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| 2 | 73 | 136 | 3,000 ms | 661,808 KiB |
| 3 | 585 | 1,672 | 2,711 ms | 590,608 KiB |
| 4 | 4,681 | 18,056 | 3,278 ms | 656,144 KiB |

The correct model had zero counterexamples in these domains. Counts describe traces, not distinct deduplicated states. The imported reservation lemma proves only that `reserve` hides counters, releases the registry flag and marks cleanup ownership for any model state. It is not an unbounded lifecycle or implementation theorem.

Fixed shortest witnesses:

- Global serialization: `beginCleanup, probeRegistry`. The broken state retains `registryHeld=true`; the unrelated probe records failure. Classification: confirmed lock-scope bug, supported separately by the Linux RED test; model correspondence remains `model-only`.
- Counter visibility (atomicity): `beginCleanup, removeDirectory, probeCounters`. The broken state has `active=true`, `directoryExists=false` before commit. Classification: unsafe proposed split, prevented by deactivation before physical cleanup; `model-only`.
- Duplicate cleanup (uniqueness): `beginCleanup, beginCleanup`. The second reservation records a violation only in the broken transition. Classification: model sensitivity witness for the root gate; `model-only`.
- Closed admission (boundary): `close, admit`. The broken transition accepts work beyond the close boundary. Classification: model sensitivity witness for closed-state publication; `model-only`.

No owner decision remains open for this model. Kernel completion semantics, partial directory-removal failures, process-group reuse, Windows notification ordering, and dispatcher cancellation use Rust tests and review rather than a claim from this model. There are no `strict` comparisons or generated-corpus comparisons in this audit.

Commands, from `formal/HoiminOracle`:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/hoimin-342-lean-model.json -- /Users/hayao/.elan/bin/lake build HoiminOracle.ResourceCleanupModel
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/hoimin-342-lean-depth2.json -- /Users/hayao/.elan/bin/lake env lean -j1 -DElab.async=false --run ResourceCleanupAuditMain.lean 2
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/hoimin-342-lean-depth3.json -- /Users/hayao/.elan/bin/lake env lean -j1 -DElab.async=false --run ResourceCleanupAuditMain.lean 3
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/hoimin-342-lean-depth4.json -- /Users/hayao/.elan/bin/lake env lean -j1 -DElab.async=false --run ResourceCleanupAuditMain.lean 4
```

The first sandboxed model-build attempt returned `monitor_error` (exit 126, 30 ms, no RSS observation). This is `infrastructure-error`, not model evidence. After permitting child-process monitoring, the same bounded build succeeded in 3,242 ms with 651,568 KiB peak RSS. No larger depth or resource limit was attempted.
