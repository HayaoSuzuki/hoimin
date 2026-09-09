# Issue #438 ranking index

`rank_candidates` now builds a local borrowed `Utf8Path` to symbol-set index from resolved target slices. Entries for repeated paths are unioned, empty symbol lists are skipped, and candidate lookup preserves the prior exact path equality and ranking-reason order.

The saved baseline at `/private/tmp/hoimin-audit2-ranking.log`, measured against base `730e68f`, recorded 10,000 candidates and 10,000 targets in 2369.921 ms, compared with 0.844 ms for the empty-target control. Both runs had identical ranking output. Those are pre-change measurements, not fixed-code timings.

The eight focused ranking tests cover resolved symbols with `Selection::default()`, duplicate target paths, multiple files, absent candidate symbols, preserved scores/ranks/reasons, and `Utf8Path` equality boundaries. An interior `.` segment matches its normalized equivalent, while `src/A.py` and `src/a.py` remain distinct on every platform. Both `cargo test -p hoimin-cli ranking_tests --lib` and `cargo test -p hoimin-cli --all-features ranking_tests --lib` passed, along with `cargo test -p hoimin-cli plan --lib`, scoped Clippy, and formatting. The controller performs the independent release-output and broad-suite comparison.
