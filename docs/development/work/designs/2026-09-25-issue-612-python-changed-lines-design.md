# Issue 612: Git and Python changed-line coordinates

A candidate inside changed source bytes must remain eligible when Python physical lines use LF, CRLF, CR, or mixed endings. Git hunk lines count LF; Python lines also recognize lone CR. The current code compares these different units directly. Base: issue 632 head f114508, so Git context expansion and deletion-gap semantics are already present.

## Design

Keep patch parsing in Git LF units. After binary/deletion exclusions, normalize the patch ranges and translate them against current raw file bytes before intersecting explicit Python line/symbol selections. A single forward scan tracks byte offset, Git LF row, and Python physical row. For each sorted disjoint Git interval, skip to its start, then consume through its end and record the first/last Python rows containing those bytes. Python advances on LF or on CR not followed by LF. CRLF counts once; no extra EOF-only row is selected. Counters use usize during scanning and checked u32 conversion for output. This needs O(source bytes + range count) time and O(range count) result memory, without duplicate per-line tables.

Untracked and unborn-indexed files have whole-file Python ranges from the start. Preserve this distinction explicitly so they are never translated as Git ranges. Empty/NUL-containing current files retain existing exclusion behavior. Tracked reads use WorkerRoot with existing missing/invalid-path handling. Before these new reads, filter eligible paths using logical_path_equality_key, retaining Windows case rules; do not intersect explicit numeric line ranges until after conversion. No changes to early Git path parsing (issue 621) or diff pathspec scoping (issue 620).

Git itself still chooses context rows via --unified=N from issue 632. Translate that resulting byte coverage; do not reinterpret N or restore old delete-only behavior when N>0. At context zero, delete-only ranges remain empty. Renames map destination contents. Binary exclusions precede current-file reads. Supported source codecs have the same raw CR/LF bytes, so no decoding or newline normalization is required. Raw spans/hashes/IDs remain the analyzer/validator's existing responsibility.

Out-of-range positive Git intervals indicate that current bytes no longer correspond to the patch (or a content transformation changed LF boundaries); return a clear error rather than silently clip/drop intervals. Standard CRLF normalization preserves LF row counts and remains supported. Arbitrary user-defined clean filters that change row counts are not modeled; preserve failure visibility rather than claim correspondence.

## Correspondence worksheet, before formal code

| Premise/observation | Lean representation | Public production observation | Mode |
| --- | --- | --- | --- |
| First/second separator | LF/CRLF/CR finite choices, original issue sources | Write exact bytes, CPython AST line | strict |
| Git state | untracked/unborn-indexed/staged-new/tracked-modified | Isolated real Git repo and real commits/index/worktree | strict |
| Candidate identity/location | Source prefix byte size, Python row, +/- spelling | Plain and --changed plan spans/hash/ID/line/column/operator | strict |
| Git LF hunk interval | Count LF in prefix; state-specific first row | Actual git diff --unified=0 hunk header for staged/tracked cases | strict |
| Eligibility | Candidate byte position lies in changed Git LF interval | Same candidate selected, regardless of Python numeric row | strict |
| Runtime effect | Original f()=3; changed minus produces -1 | Public run counts one killed mutant, baseline successful | strict |

Retain all 36 original cases (3×3 separators×4 Git states), including the 20 old numeric-coordinate failures. The model proves only local CRLF transition behavior and checks this bounded matrix. Keep deliberately broken LF-only and CR/LF-double-count witnesses. Atomicity/idempotency/concurrency do not apply to this pure coordinate contract. Negative selection, deletion, rename, binary, explicit intersections, attributes, other codecs and saved-plan verify are public Rust regressions beyond the finite model; do not label them Lean-proved.

## Design self-reviews before implementation

1. Unit separation: a common postprocessing pass would mistakenly reinterpret unborn physical ranges as Git LF ranges. Translate only patch-derived ranges, then union whole-file physical ranges. A byte scanner must assign CRLF's final LF byte to the preceding physical row; use lookahead on CR and update row counters after observing each byte.
2. Selection/platform boundaries: filtering by exact path spelling would break Windows case-equivalent Git/discovered paths. Use the existing logical equality key and filter paths only; applying explicit line intersections before translation would recreate the bug. Binary/deletion exclusions must occur before new file reads.
3. Scope/resource review: building both LF and Python line-start vectors would duplicate large indexes. Choose a monotone byte scan after range normalization. Preserve 632's positive-context deletion coverage and keep 620/621 separate. Check actual Git hunks and CPython observations so corpus comparisons do not merely repeat production arithmetic.
