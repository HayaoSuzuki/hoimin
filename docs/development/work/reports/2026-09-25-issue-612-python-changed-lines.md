# Issue 612: changed-line coordinate correspondence and fix

Design/plan and three pre-code reviews each were committed as `822f716`, on issue 632 head `f114508`. The implementation retains Git hunk/context selection, then maps that raw-byte coverage to Python physical rows. Unborn-indexed and untracked whole-file ranges start in Python units and are not translated again. No source bytes, candidate identity, or schema is changed.

## Claim, boundary, and correspondence

A mutation whose bytes lie inside a selected Git LF interval must remain selected even when its Python row number is larger because of lone CR separators. The pre-model worksheet is in `docs/development/work/designs/2026-09-25-issue-612-python-changed-lines-design.md`. The committed Lean domain is the original issue matrix: 3 first separators × 3 second separators × 4 actual Git states = 36 strict cases, at most 37 ASCII bytes each. All expected eligibility/rows/offsets come from the generator; adapters write exact source bytes, inspect CPython AST lines, inspect real Git hunks, and invoke public plan/run/verify. Ranking fields intentionally differ under --changed, so comparison uses candidate identity/location fields only.

The model proves `crlfOne` for every initial row and previous-CR flag. Its row observations are token positions after separators, not positions inside the two bytes of CRLF. It checks a finite source domain, not Rust/Git/CPython implementation correctness. Three deliberately broken alternatives detect LF-only counting, CR/LF double counting, and direct comparison of Python rows to Git intervals. Atomicity, idempotency and concurrency are inapplicable to this pure coordinate claim. Binary/rename, other codecs/BOM, explicit intersections, saved-plan replay and attributes are additional public regression tests, not model proofs.

## RED → GREEN evidence

Baseline existing target handler: 45 passed; issue 632 changed-context adapter: two tests passed. The new 36-case public adapter then reproduced exactly 20 semantic mismatches: every CR-containing separator pair in all four Git states lost the one candidate. All 36 plain plans and CPython/model/Git observations matched their premises. A representative original fault is CR/CR source `# header\rdef f():\r    return 1 + 2\n`: the candidate is Python line 3 but Git row 1; numeric comparison drops it. New public run also failed with killed=0 instead of 1 for staged-new LF/CR. Corpus schema/duplicate checks already passed. Evidence: `612-red-public.log`.

After the fix, all 36 strict plan cases match, and six representative public run/saved-plan-verify cases cover LF, CRLF, CR and mixed endings across all four Git states. Baselines succeed, one mutant is killed, the run is complete, saved-plan candidate IDs agree, and source bytes remain intact. Final correspondence has 36 matches, zero semantic mismatches and zero infrastructure errors. The shared adapter enforces 20-second subprocess deadlines and a 40-second public CLI deadline; run configuration retains a 20-second total timeout.

The pure scanner additionally matches the existing core physical-line-start contract for all 1,365 byte strings of length 0–5 over x/CR/LF/0xe9 and their 4,304 nonempty Git intervals. Dedicated tests cover disjoint ranges, empty/trailing-EOF rows, invalid/out-of-source intervals and u32 result overflow. This is an independent Rust differential check, not a Lean theorem.

## Implementation reviews

1. **Byte boundaries:** re-read cursor updates against core line-start semantics. Capture the row before advancing the byte; CR advances only if the next byte is not LF. Track the last actually consumed Git row to reject a fictitious empty EOF Git line. Disjoint normalized intervals advance one cursor without rescanning prefixes. Differential boundary tests pass; no additional production correction was required.
2. **Units, paths, and I/O:** a patch-derived flag protects unborn physical ranges from double conversion. Binary/deletion exclusions occur first; eligible paths use the existing logical equality key before additional reads, preserving Windows case rules. Numeric explicit intersections remain after translation. Missing/invalid-path handling is shared unchanged with the old untracked reader. Early unrelated-path validation (621) and Git diff scoping (620) remain outside this fix.
3. **Context and compatibility:** positive --changed-context still comes from Git's --unified expansion, including deletion-gap neighbors; context zero keeps delete-only selections empty. Translation uses current destination bytes for renames. Supported codecs share raw CR/LF bytes and need no decoding here. Out-of-range positive intervals now expose mismatching patch/current-byte premises as a clear error instead of silently clipping or dropping lines. Arbitrary clean filters that change LF row counts remain outside the model; standard text/eol attributes are tested.

## Test reviews and corrected preparation findings

1. **Oracle independence and sensitivity:** original runtime eligibility stayed unchanged; all 20 historical failures were observed before production changes. CPython AST and actual Git hunks corroborate the model, and the adapter compares stable candidate fields instead of ranking scores. Three broken-model families remain executable and all detect their fixed witnesses.
2. **Cross-contract coverage:** existing changed-target and changed-context oracles remain green. Added explicit physical-line intersection, positive-context CR deletion, rename+binary+text-attribute checks, and Latin-1/BOM public plan/run checks with positive/negative line/symbol intersections. The first rename fixture was too short for Git similarity detection: actual diff showed separate new/deleted files and correctly selected all new rows. Increased unchanged content and asserted the rename header; this fixes the test premise without weakening the expected translated range. Logs: `612-rename-premise.log`, `612-all-focused.log`.
3. **Operational isolation:** each corpus case uses a fresh repository, controlled Git configuration, exact writes and bounded subprocesses; setup errors are not counted as semantic mismatches. Added generator beside changed_context in lake/CI/Python registry, plus model/main build-module entries; all 40 Python workflow contract tests pass. Initial Clippy found only a similar test binding name and CPython documentation formatting; both were corrected. No Python production behavior changed, so mutation testing of Python production is inapplicable.

Root independently reviewed `git.rs` plus the new `git/lines.rs` and found no blockers: cursor CRLF ownership, bare-CR expansion, normalized forward traversal, strict EOF handling, eligible filtering and unborn unit separation were checked.

## Lean resource and freshness evidence

Every command used one globally authorized Lean lane, 20 seconds / 2048 MiB external guard, and local theorem heartbeats 10000. No bound increase, timeout, OOM or Lean setup failure occurred. Model check: 2.784 s / sampled peak 616,400 KiB. Generation: 2.767 s / 683,728 KiB. Native executable build+freshness: 5.022 s / 692,144 KiB. Native sensitivity/stats each completed in about 0.3 s; these very short processes are not useful peak-memory samples. Domain: 36 cases, 9 separator pairs, 4 Git states, maximum source 37 bytes; no transition alphabet/depth search.

From `formal/HoiminOracle`, exact command forms were:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-batch-604-632/612-lean-model-stats.json -- lake env lean -j1 -DElab.async=false -o .lake/build/lib/lean/HoiminOracle/ChangedLinesModel.olean HoiminOracle/ChangedLinesModel.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-batch-604-632/612-lean-generate-stats.json -- lake env lean -j1 -DElab.async=false --run ChangedLinesAuditMain.lean --output corpus/changed-lines.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-batch-604-632/612-lean-fresh-stats.json -- lake exe generate_changed_lines -- --check corpus/changed-lines.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-batch-604-632/612-lean-sensitivity-stats.json -- lake exe generate_changed_lines -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-batch-604-632/612-lean-domain-stats.json -- lake exe generate_changed_lines -- --stats
```

The checked-in corpus was generated by Lean and checked by the native entrypoint; no expected values were edited by hand. All resource logs and RED/GREEN artifacts remain under `/tmp/hoimin-batch-604-632/`.

## Workspace validation

Before main integration, `cargo test --locked --workspace` passed 2,333 tests with zero failures and 22 ignored tests across 100 result entries. Both exact CI Clippy commands passed: `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`. Workspace/vendor formatting, `git diff --check`, the 40 Python workflow contract tests, and Ruff on the changed Python registry passed. Logs: `612-workspace.log`, `612-clippy-final.log`, `612-clippy-vendor.log`, `612-ci-contract.log`.

The unpublished two-commit branch rebased cleanly onto main `421f5ef`, excluding the merged 632 parent `f114508`. Final integration reran target_handler plus all three changed-selection oracle suites: 61 tests passed, including the new public plan/run/verify and BOM/Latin-1 cases. Both exact Clippy gates, both formatting gates, all 40 Python CI registry tests, and diff whitespace checks passed on the integrated tree. Root explicitly requested focused integration after the clean rebase rather than an unconditional duplicate full-suite run; no production conflict or unresolved integration concern remained. Logs: `612-rebased-focused.log`, `612-rebased-clippy.log`, `612-rebased-clippy-vendor.log`, `612-rebased-ci-contract.log`.

After PR #650 initially passed all CI, main `195773a` was merged into the published branch without rewriting history. Only the two Lean CI build-module lists conflicted; both ChangedLines and GlobSelection entries were retained. Generator/lake/Python registry order merged consistently. Five target/oracle integration suites (63 tests), 40 Python CI contract tests, both exact Clippy gates, both formatting gates and whitespace checks passed after this merge. Lean models/corpora were unchanged. Evidence: `612-main-merge-*.log`.

## Raw byte row conversion

The row model tokenizes CRLF as a single delimiter, then partitions the same
token stream in two ways. Git rows end at LF or CRLF. Python rows also end at
a standalone CR. Delimiters belong to the row they end, so a terminal newline
does not create an additional empty row. A Git range maps to the first Python
row in its starting Git row and the last Python row in its ending Git row.
The conversion counts skipped Git rows when locating a later range.

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Current source | Raw bytes classified as payload, CR, or LF | Bytes passed unchanged to `physical_line_count` and `translate` |
| Requested Git rows | Ordered list of one-based inclusive intervals | Typed `LineRange` values at the existing cursor seam |
| Complete outcome | Count, translated intervals, or first diagnostic | Rust count and ordered range list or exact `TargetError::GitFailed` |

Lean proves a general partition theorem. When one delimiter set includes the
other, cutting each outer row with the larger set yields the same rows as a
single cut over the input. Applied here, each Git row contains at least one
Python row, and their counts sum to the physical line count. Prefix counts are
monotone, so disjoint ordered Git ranges translate to disjoint ordered Python
ranges. A valid nonempty Git range always maps to a nonempty span within the
physical line count. These theorems have no bound on the number of bytes or rows.

The 374 `internal-fixture` cases include every byte sequence of length zero
through five over payload, CR, and LF. For each, the corpus checks the empty
query, all single intervals with endpoints from zero through one past the last
Git row, and every pair of valid ordered disjoint intervals. The total is 8,139
queries. Ten additional cases include skipped standalone CRs, overlaps, rewinds,
`u32::MAX` endpoints, competing diagnostics, and bytes outside the three-symbol
domain. The owned cursor rejects malformed intervals and ranges beyond the
current bytes. Overlaps and rewinds exercise the internal seam. Public target
resolution supplies normalized ranges. The existing public changed-line oracle
also covers candidate selection from real Git repositories.

Six sensitivity witnesses detect counting LF only, double-counting CRLF,
adding a phantom trailing row, losing CRs in skipped rows, allowing the cursor
to rewind, and clipping intervals to EOF.

Rust matched all 374 source counts and all 8,139 query results. On macOS,
113 Git unit tests and 43 integration tests passed. The final Lean checks ran
serially with a 20-second timeout and 2 GiB RSS ceiling and peaked at
1,288,032 KiB. The single-job Rust checks peaked at 2,160,880 KiB under a
3 GiB ceiling.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-physical-rows corpus/git-physical-rows.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --physical-rows-stats)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope --test lean_changed_lines_oracle
```

## Current worktree collection

The collector for untracked paths and the unborn index first decodes the whole
NUL-delimited inventory. An empty explicit scope still validates its encoding
and then skips opening the repository root. A scoped target authorizes a read
when its path appears in the inventory and it includes at least one positive
line. A target without explicit lines authorizes the whole file. Each regular,
nonempty file without NUL bytes contributes its complete physical row range.
The final target intersection applies explicit line restrictions after collection.

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Inventory and scope | Typed path records and optional target slices | Exact bytes and target slices passed to `collect_current_worktree_paths` |
| Current root and paths | Available or absent root, and path entries for contents, directory or blocked parent | Isolated temporary filesystem and real `PortableFileReader` |
| Success | Existing changes followed by complete physical spans for collected files | Complete ordered range vectors in the resulting map |
| Failure | First diagnostic and original changes | Stable operation and path from `TargetError::GitFailed` and the full map |

Lean proves that an empty scope still validates the inventory, a regular
nonempty file with no NUL contributes its full physical span, and a fatal read
after any successful prefix returns that failure with the original change map.
The five sensitivity checks detect skipping validation for empty scope, skipping scope,
intersecting ranges at read time, including NUL-bearing content, and publishing
partial results as files are read.

The 134 `internal-fixture` cases include 120 combinations of scope, path lists,
and file states, plus 14 named cases for validation order, root availability,
and failure atomicity. Three cases use a regular file in place of a parent
directory to reproduce a fatal parent-open error on Unix. Their Rust adapter
runs on Unix. The remaining cases use portable fixtures. Git inventory
production, platform case aliases, and concurrent source edits fall outside
this model. The public integration suites exercise real Git inventories and
their subsequent target intersection.

On macOS, all 125 success cases and nine failure cases matched Rust. The Git
unit suite passed 114 tests, and the three integration suites passed 57 tests.
Lean commands ran serially under a 20-second deadline and 2 GiB RSS limit and
peaked at 1,237,760 KiB. The single-job Rust checks peaked at 2,316,336 KiB under
a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-git-worktree corpus/git-worktree.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope --test changed_excluded_paths --test lean_changed_lines_oracle
```
