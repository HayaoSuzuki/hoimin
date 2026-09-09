# Issue #438 ranking index

`rank_candidates` now builds a local borrowed `Utf8Path` to symbol-set index from resolved target slices. Entries for repeated paths are unioned, empty symbol lists are skipped, and candidate lookup preserves the prior exact path equality and ranking-reason order.

The saved baseline at `/private/tmp/hoimin-audit2-ranking.log`, measured against base `730e68f`, recorded 10,000 candidates and 10,000 targets in 2369.921 ms, compared with 0.844 ms for the empty-target control. Both runs had identical ranking output. Those are pre-change measurements, not fixed-code timings.

Focused ranking tests cover resolved symbols with `Selection::default()`, duplicate target paths, multiple files, absent candidate symbols, and preserved scores, ranks, and reasons. `cargo test -p hoimin-cli ranking_tests --lib`, `cargo test -p hoimin-cli plan --lib`, scoped Clippy, and formatting passed. The controller performs the independent release-output and broad-suite comparison.
