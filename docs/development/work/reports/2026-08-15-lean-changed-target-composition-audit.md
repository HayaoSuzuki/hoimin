# Changed-target range composition Lean audit

## Verdict

Issue #313 found one same-premise Rust mismatch. A changed candidate inside an
explicit symbol selector was absent from public `plan`. Target resolution had
correctly resolved `a:Widget.run` to `pkg/a.py`, but transported only
`Widget.run`; the analyzer required a `module:qualname` string and rejected every
qualname-only selector.

The retained regression creates a Git repository where changed lines exist in
both `Widget.run` and another function, then runs public
`plan --changed --symbol a:Widget.run`. Before the repair it returned no
candidates. The minimal repair makes the analyzer accept the resolver's owned
qualname form while retaining support for module-qualified internal requests.
It does not change the public schema or target representation.

## Formal contract

Lean models parsed semantic change facts rather than unified-diff bytes. A fact
records its kind, source and destination paths, destination line ranges, and
current line count. Range normalization is specified extensionally as membership
in valid positive one-based inclusive ranges. `normalizedLines` materializes that
set within an explicit bound as an ordered, duplicate-free list. Invalid ranges
vanish and overlapping or adjacent ranges have the same canonical line-list
representation as their merged form. Rust tests separately pin concrete range
materialization.

The kernel-checked modules prove:

- bounded normalization preserves membership and its canonical line-list output
  is idempotent;
- changed/explicit intersection is commutative at observation level and a
  subset of both inputs;
- intersection cannot create a path absent from the changed facts;
- deletion and binary exclusion dominate positive facts;
- rename selection uses the destination and cannot select a distinct source;
- an untracked selection is bounded by `1..currentLineCount`;
- empty explicit selection is the identity for changed selection.

The external consumer imports only the model and proof modules. Cases,
sensitivity checks, corpus generation, and Rust fixtures stay outside the proof
dependency boundary.

## Correspondence worksheet

The closed corpus contains eleven rows. Each row is rendered from a typed Lean
`AuditCase`; `eligible_lines` is computed by enumerating `CombinedEligible`, not
stored as an expected literal. Rust looks up each strict row by ID and compares
the public observation with those generated lines.

| Mode | Rows | Observation |
| --- | ---: | --- |
| `strict` | 8 | real Git fixture and public plan candidates |
| `internal-fixture` | 1 | owned parser-isolation coverage |
| `model-only` | 1 | non-UTF-8 logical path not representable publicly |
| `infrastructure-error` | 1 | Git/setup failure classification only |

Strict cases cover contiguous modified lines, explicit line intersection,
symbol intersection, rename destination attribution, deleted and binary
exclusion, an unterminated untracked last line, and `--diff-base` composition of
committed HEAD plus worktree edits. Candidate observations check path, line,
source byte span, original/replacement/operator, a source-derived BLAKE3 file
hash, symbol where applicable, and a stable ID recomputed only after those
fields are independently validated. The deletion/binary fixture also inspects
owned `ResolveGitChanges` output and raw Git name/numstat premises, so zero
candidates cannot hide a leaked target.

The original changed-target model takes parsed facts as input. Existing Rust
property tests generate zero-context diff bodies. The regression corresponding
to the `hostile-parser` row shows that structural-looking body lines in one
section do not contaminate the later valid Python section. Integration tests pin
destination hunk coordinates, rename configuration, binary numstat, deletion,
and staged/unstaged cancellation. The patch transport audit below covers the
file-section boundary and destination hunk ranges directly.

## Sensitivity and bounded cases

All ten explicit faulty transports are distinguished from their canonical
observations: failure to merge adjacency,
merging across a one-line gap, old-side coordinates, deleted/binary retention,
source-side rename attribution, untracked final-line loss, union instead of
intersection, dropped symbol restriction, pre-normalization path membership,
and contamination of a later valid section. Eight fixed semantic cases and ten
sensitivity families run within the recorded bound of two facts and two ranges.
The generated bounded range-set enumeration evaluates preservation and
idempotence across 1,365 membership states; `--stats` fails if any state fails.
Enumeration is evidence, not proof.

## Counterexample ledger

| Field | Observation |
| --- | --- |
| Premise | changed line 3 is inside `pkg/a.py`, symbol is `Widget.run` |
| Explicit selector | `a:Widget.run` resolves to `pkg/a.py` |
| Lean | combined observation at line 3 is eligible |
| Rust before repair | public plan has zero candidates |
| Cause | resolver transports `Widget.run`; analyzer rejected strings without `:` |
| Repair | interpret qualname-only selectors at the already resolved target path |
| Regression | `public_plan_preserves_symbol_restrictions_under_changed_intersection` |

## Resource measurements

Each retained Lean command ran alone with a 20,000 ms deadline, a 786,432 KiB
root-plus-descendant RSS ceiling, and 250 ms sampling.

| Command | Elapsed ms | Peak RSS KiB | Exit / reason |
| --- | ---: | ---: | --- |
| proof module direct compile | 1,630 | 573,872 | 0 / `child_exit` |
| external proof consumer | 565 | 54,512 | 0 / `child_exit` |
| ten sensitivity families | 292 | 224 | 0 / `child_exit` |
| eight fixed cases | 288 | 2,672 | 0 / `child_exit` |
| corpus freshness | 291 | 2,112 | 0 / `child_exit` |

No retained command reached either limit. Peak RSS remained 212,560 KiB below
the ceiling.

## Exclusions

Git merge-base correctness, Git rename detection itself, a byte-for-byte diff
grammar proof, candidate ranking after eligibility, and filesystem races after
Git observation remain outside this audit.

## Verification commands

```text
lake build
lake env lean HoiminOracle/ChangedTargetProofs.lean
lake env lean /tmp/hoimin-changed-target-proof-consumer.lean
lake exe generate_changed_target -- --cases
lake exe generate_changed_target -- --sensitivity
lake exe generate_changed_target -- --stats
lake exe generate_changed_target -- --check corpus/changed-target-composition.jsonl
cargo test -p hoimin-core --test target_policy --all-features
cargo test -p hoimin-cli --test target_handler --all-features
cargo test -p hoimin-cli --test lean_changed_target_oracle --all-features
cargo test -p hoimin-cli --test plan --all-features
cargo test --workspace --all-features
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

## Diff selection configuration

The configuration model covers `--changed`, `--changed-context`, and
`--diff-base` alongside the four explicit selector types. Lean proves that
accepted changed selection requires a source and that twice the maximum
accepted context fits a signed 32-bit value.

The 386 fixtures distinguish three boundaries. The CLI rejects an explicit
context without `--changed`, including zero, and checks the numeric range
before constructing run settings. Raw configuration treats omitted and
explicit zero context alike. It checks the context bound before configuration
dependencies. Saved settings carry one optional changed scope. Deserialization
checks its context bound and normalized run construction checks the presence
of a source. An explicit revision string is retained verbatim.

The Rust adapter compares both public CLI parsers, raw construction, saved
run settings, and plan restoration. It verifies the normalized scope or the
first diagnostic at each boundary. The sensitivity check detects dropping
explicit zero, checking the source before the context bound in raw settings,
and ignoring a supplied revision. The comparison ends before Git resolves
revisions or computes changed lines.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-config corpus/changed-config.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test cli_config changed_selection_configuration_matches_lean
```

## Normalizing target selector paths

An absolute selector must match the project root component by component before
its remaining path is reduced. A relative selector starts directly at the root.
The reduction skips empty and dot components, removes one accumulated component
for each `..`, and rejects a parent step at the root. A selector that names the
root produces an empty logical path. The source selector interprets that value
as selecting the whole discovered tree.

| Observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Absolute path origin | Declared root and input components with platform-specific equality | Public `normalize_logical_path` receives the rendered native spellings |
| Relative normalization | The same component reduction used by the import-root model | Exact result string or `PathOutsideRoot` carrying the original spelling |
| Target selection | Eligible Python path, empty logical root, or an invalid source | `resolve_explicit` on the normalized fixture's discovery inventory |

Lean proves that stripping a matching root preserves the suffix, that a
nonmatching component fails the comparison, and that the root itself reduces
to the empty logical path. Once reduction tries to leave the root, appending
more components cannot repair the failure. Sensitivity checks detect partial
component matches, clamping `..` at the root, switching case rules on either
platform, and rejecting the empty logical result.

The corpus has 240 cases. The common cases include every sequence of up to
three components drawn from two ordinary spellings, the empty component, `.`,
and `..`. Each platform has a shorter absolute-path matrix and targeted
controls for component boundaries and native spellings. The Windows cases
include drive prefixes, UNC shares, and three representative Unicode case
pairs. The case model supplies component comparisons for those spellings.
The assertions concern lexical normalization and resolved targets. Filesystem
identity and symlink resolution are outside this model.

Rust on macOS matched 135 accepted and 62 rejected cases. The additional 43
cases apply to Windows. The target and candidate policy suites passed 51 and
92 tests, the target handler suite passed 70, and the CI workflow suite passed
222. The longest input was 31 UTF-8 bytes. Guarded Lean checks peaked at
1,319,360 KiB under a 2 GiB limit and 20-second timeout. Single-job Rust checks
peaked at 966,528 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_environment_fingerprint -- --check-target-path corpus/target-path.jsonl)
(cd formal/HoiminOracle && lake exe generate_environment_fingerprint -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test target_policy --test candidate_policy
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test target_handler
```

## Pruning discovery to selected paths

The discovery filter retains every descendant of a source, the exact paths
named by file and line selectors, and the directories leading to those paths.
Source containment compares complete path components. Built-in exclusions
apply even when a selector names an excluded subtree. A project-root source,
a symbol request without a source, an invalid path normalization, or an empty
selection leaves discovery unscoped at the filesystem function boundary.
The caller can diagnose selector errors before or after discovery according
to the selector type.

| Observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Scope construction | Source paths, file paths, line paths, symbol presence, and path reduction | Actual `DiscoveryScope::new` on the fixture selection |
| Paths inspected by the filter | An entry is reached only when every ancestor directory remains traversable | `DiscoveryStats.visited_entries` from the real walk |
| Discovered records | Reachable, admitted regular files and their Python flags | Complete result of `discover_explicit_with_stats` |
| Unsupported native spelling | A literal backslash in a Unix path | Real Unix filename and exact `UnsupportedPath` value |

Lean proves that a selected source admits its descendants, exact selectors
retain their ancestor directories, and a rejected ancestor prevents traversal
to its children. The 72 matrix cases combine six source configurations,
six exact configurations, and the presence or absence of a symbol. Nine focused
cases cover a source naming a file, normalization to the project root, an exact
selector naming a directory, native spellings, and explicit built-in exclusions.
Five cases require Unix filename semantics. Each diagnostic fixture contains
one unsupported path, so directory enumeration order cannot change the
expected error. The corpus measures complete filter visits for successful walks.
The initial model counted the walk root and predicted 17 calls for the broad
fixture. Rust observed 16. The walker accepts its root before calling the
filter, so the model counts only entries below the root.

The six sensitivity checks detect omitting line selectors, pruning symbol
discovery without a source, using a raw string prefix for containment, dropping
the directories of exact paths, ignoring built-in exclusions, and losing the
malformed-file diagnostic. Fixtures use a plain temporary tree and exercise
the pruning filter directly. Ignore patterns, include restoration, symlink
traversal, and non-UTF-8 directory entries have separate Rust coverage.

All 81 cases matched on macOS. The 78 successful walks visited between five
and 16 entries, and the three errors carried the expected native path. Rust
passed 27 discovery unit tests, with three existing benchmarks ignored, and
89 integration tests. The CI workflow suite passed 222 tests. The final guarded
Lean checks peaked at 1,291,296 KiB under a 2 GiB limit and 20-second timeout.
Single-job Rust checks peaked at 1,733,616 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-discovery-scope corpus/discovery-scope.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::fs
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test target_handler --test lean_glob_selection_oracle
```

## Bounded Python file discovery

Exception hierarchy preparation collects distinct Python file paths under a
single limit across the normal walk and the include restoration walk. The
limit permits the last available slot. A path already collected can recur
when capacity is full. A file first yielded by restoration uses the same
budget as a file first yielded by the normal walk. The collector checks
cancellation once per yielded entry before handling a walk error or testing
the entry's type. Directories and non-Python files use no file slot.

| Observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Budget across passes | A single accumulated path list threaded through both entry streams | Real normal and include walks use the same collection |
| Cancellation | One-based callback ordinal with sticky termination | Closure counts actual cancellation polls and returns true at the selected ordinal |
| Path admission | Eligible Python, other entry, unsupported native spelling, or walk failure | Temporary files, directory tree, ignore controls, and absent root |
| Final result | Sorted unique paths or the first failure, with poll count | The actual `discover_python_bounded` result and observed callback count |

Lean proves that every successful scan returns unique paths within the limit,
that revisiting a path preserves the set even at capacity, and that a
cancellation at the next poll precedes every possible entry result. A failed
state stays terminal. Rust exposes one `AnalysisLimit` diagnostic for both
cancellation and capacity exhaustion, so the test compares that public error
and the observed poll count.

The 240 cases combine ten stream shapes, limits zero through three, and six
cancellation schedules from no cancellation through cancellation at poll
five. The longest stream has four entries. The two-file fixture has two
distinct Python files, so its observation is independent of directory
enumeration order. Real ignore and include rules deliver the restored-file
cases. The Unix-only cases use a literal backslash in a Python or non-Python
filename. The model starts at the entries yielded by the walker and abstracts
the contents of traversal diagnostics. The five sensitivity checks detect
charging duplicates, renewing capacity at restoration, skipping polls on
other entries, reporting an entry error before cancellation, and validating a
non-Python filename.

All 240 cases matched on macOS. The collector returned 89 successful path
lists, 115 `AnalysisLimit` errors, 20 walk errors, and 16 unsupported-path
errors. Rust passed 28 discovery unit tests with three existing benchmarks
ignored, and 87 analyzer and fingerprint integration tests. The CI workflow
suite passed 222. Guarded Lean checks peaked at 1,341,200 KiB under a 2 GiB
limit and 20-second timeout. Single-job Rust checks peaked at 1,738,240 KiB
under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-bounded-file-discovery corpus/bounded-file-discovery.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::fs
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test fingerprint_inputs --test analyzer_handler
```

## Resolving symbol modules and definitions

Hoimin resolves a dotted module by joining its name components with slashes
under each source in the supplied order. Within a source it tries `module.py`
first and `module/__init__.py` second, using the first eligible Python file.
The selected file receives the qualified symbol name. Other discovered Python
files under the sources retain their whole-file selection. Several selectors
on one file produce a sorted, deduplicated list of qualified names. The output
orders files lexicographically by their path components.

| Observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Module candidates | Ordered source roots, two probes per source, Python admission flag | `resolve_explicit` consumes a declared discovery inventory |
| Attached selectors | Ordered name lookup and bindings grouped by resolved path | Exact `TargetSlice` list, including files without attached names |
| Missing module | First unresolved selector | `SymbolModuleNotFound` with the unresolved module |
| Missing definition | Definition membership in the selected file | `TargetHandler::resolve` reads Python files and reports the exact diagnostic |

Lean proves that an eligible module file wins over its package in the same
source, that an eligible package wins over all later sources, that a source
without an eligible candidate is skipped, and that the first unresolved
selector fails the resolution. The 172 cases include both orders of two
source roots and all absent, regular, and Python states for their four
candidate paths. Ten focused cases cover dotted modules, source boundaries,
the project-root selector, repeated names, errors, and definition validation.

Three cases also run discovery and definition validation on real Python files.
The same requested function exists in a package and in a later source. The
argument order and available files determine which file is read for the
definition. When the first module file lacks the function, validation reports
that file even though other candidates define it. Reordering the roots or
removing that file permits the corresponding later-source or package case to
succeed. The corpus uses canonical portable path spellings and predeclared
definition names. The runtime adapter checks those names with Hoimin's Python
parser.

The seven sensitivity checks detect reversing the roots, preferring packages
within one source, preferring all module files before any package, accepting
a regular non-Python record, dropping the leading parts of a dotted module,
ordering the entire path as a flat string, and accepting a definition from
another file. The initial Lean model ordered paths as flat strings, so its
target list placed `second/calc.py` ahead of `second/calc/__init__.py`. The
model now compares components and retains this matrix case as a witness.

Rust on macOS matched 137 successful target lists and 35 missing-module
errors. The three real-file cases produced two target lists and one
missing-definition error, as predicted. The largest fixture has six discovered
paths. Core target tests passed 52, CLI integration tests passed 82, and CI
workflow tests passed 222. Guarded Lean checks peaked at 1,272,160 KiB under
a 2 GiB limit and 20-second timeout. Single-job Rust checks peaked at
958,384 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-symbol-resolution corpus/symbol-resolution.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test target_policy
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test target_handler --test lean_changed_target_oracle
```

## Line selection coverage

The Lean model treats each selection as an inclusive interval. It proves
that a query is covered exactly when its start and every in-query line
immediately after an input interval are covered. This check uses the same
number of boundary probes for a narrow interval and one ending at
`u32::MAX`. A separate theorem shows that merging overlapping or adjacent
intervals preserves the selected lines.

The corpus checks all 256 ordered pairs of intervals with endpoints in
0 through 3 against all 16 queries in that domain. Twelve additional cases
cover long intervals, file isolation, unsorted input, duplicates, and files
with multiple target slices. An empty or uncovered slice disqualifies its
file even when another slice is covered. The corpus also checks the core
boundary where typed explicit selections can contain 0 and changed-line
normalization discards ranges starting at 0.

The Rust adapter calls `LineSelectionIndex` and `normalize_changed` with
canonical relative paths. It compares range coverage, membership of the
query start, normalized changed intervals, and the covered path set.
Sensitivity witnesses detect checking only endpoints, requiring a single
input interval to cover the query, and accepting a file with any one covered
slice. Path normalization and Git diff parsing are outside this comparison.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-line-coverage corpus/line-coverage.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test target_policy line_selection_coverage_matches_lean
```

## Tracked diff inventory and pathspec batches

Scoped change resolution uses Git's name-status inventory to decide how to
fetch patch and numstat records. Only the status destination participates in
scope membership. A selected rename, copy, unusual status, or path exceeding
8,192 UTF-8 bytes requires the full diff. Other selected paths are sorted,
deduplicated, and packed into the longest successive prefixes that fit both
128 paths and 8,192 path bytes. The byte budget counts path spellings without
the fixed Git flags or argument separators.

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Recognized status and path fields | Typed record and NUL-delimited rendering | The actual `inventory_plan` parses the rendered bytes |
| Resolved selection | Scope of canonical relative paths | `GitPathScope` built from the same target paths |
| Fallback or batches | Branch and complete ordered batch contents | Exact `DiffPlan` equality at the owned parser seam |

The initial 50 cases use `internal-fixture` mode. The matrix crosses all nine status
kinds with four scopes, and 14 additional cases exercise empty input and scope,
duplicates, ordering, separator aliases, and the count and byte boundaries.
In particular, the fixture with two UTF-8 names fits under the character count
but requires two batches by byte count. Synthetic long path spellings exercise
the planner without creating files. Case aliases, malformed record diagnostics,
Git's rename detection, and subprocess execution remain outside the model.
Existing `changed_git_scope` integration tests exercise real Git commands and
the public plan.

Lean proves the fallback criterion and the maximality of each selected prefix.
For any input whose individual paths fit, concatenating the batches recovers
the original sequence, and every batch is nonempty and meets both budgets.
It also proves that the scoped plan contains exactly the paths in both the
inventory destinations and the scope.
Sensitivity witnesses detect using the source path, checking fallback before
scope, keeping duplicates, treating either limit as exclusive, counting
characters, and omitting the oversized-path fallback. These are pure planning
decisions, with no concurrent or durable effects in the model.

All 50 plans matched in the initial audit. On macOS, the focused Rust Git unit
suite passed 110 tests and the real-Git scope suite passed 19. Lean commands ran
serially with a 20-second deadline and a 2 GiB process-tree RSS limit. The
final Lean checks peaked at 1,212,464 KiB, with a longest command of 5.4 s.
The single-job Rust checks peaked at 2,250,816 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-git-inventory corpus/git-inventory.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git::tracked
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope
```

## Tracked ranges against current source

For tracked paths, the resolver filters the normalized Git ranges by path
before opening the worktree. It returns an empty map when no paths remain.
For each selected path, it reads current bytes through the retained root.
Missing files, directories, symlinks, empty files, and files with a NUL byte
contribute no ranges. For other files, it converts Git LF rows to Python
physical rows before applying any line selector. A read failure or a range
outside the current bytes aborts the conversion. The resolver reports the
first failure in path component order and returns no partial map.

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Tracked paths and ranges | Unique portable path keys with normalized Git spans | `GitChangedRanges::from_patch`, with no additional binary exclusions |
| Path selection | Optional list of resolved targets, including explicit line selectors | `GitChangedRanges::into_physical` receives actual `TargetSlice` inputs |
| Current source | Absent root or per-path entry with bytes or a file kind | Temporary worktree read through `PortableFileReader` |
| Projection | Existing Git-to-Python physical row model | Exact output map or typed error from `into_physical` |

Lean proves that scope filtering preserves input membership, an empty scope
succeeds without a root, skipped files bypass row conversion, and a late
failure returns an error even after a successful prefix. The 144 matrix cases
combine six scopes, eight source states, and three normalized range patterns
over two paths. The initial ten focused cases cover missing roots, line selection,
late failures, competing diagnostics, and live and dangling symlinks. Four
cases use Unix-specific filesystem behavior. The six sensitivity checks cover
ignoring scope, opening the root prematurely, keeping Git line numbers,
accepting NUL bytes, returning partial results after stale ranges, and reporting
errors in unsorted input order.

The model uses static source observations and portable ASCII path spelling.
The fixture for a stale range supplies shortened current bytes directly. The
worktree read and the range conversion run through production code. Concurrent
changes during a read and platform-specific case equivalence are outside
these cases.

In the initial audit, Rust matched 126 successful maps and 28 errors on macOS.
The Git unit suite passed 122 tests, the two changed-target integration suites passed 37,
and the CI workflow suite passed 222. The longest source fixture was 12 bytes.
Guarded Lean checks peaked at 1,264,048 KiB under a 2 GiB limit and a 20-second
timeout. Single-job Rust checks peaked at 2,342,512 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-git-tracked-rows corpus/git-tracked-rows.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope --test lean_changed_target_oracle
```

### Path order and the first diagnostic

Lean and Rust initially disagreed when reading `pkg/a/child.py` failed and a
range in `pkg/a.py` extended beyond its current source. Rust reported the
child path's read error. The Lean model compared entire path strings and
predicted the range error first. It now shares the component comparator used
by the symbol resolution model. Lean proves that a shared ancestor prefix
leaves the comparison unchanged and that an ancestor precedes a descendant.

| Operation | Order observed in Rust | Added cases |
| --- | --- | --- |
| Tracked range projection | Components of keys in the `Utf8PathBuf` map | Two competing errors with their kinds swapped, plus scope excluding the earlier error |
| Unborn or untracked worktree reads | Git record order without a scope, resolved target order with a scope | The same two blocked paths in both modes |
| Literal Git pathspec batches | Full strings in the `BTreeSet<String>` | One batch containing `pkg/a.py` and `pkg/a/child.py` |

The new sensitivity checks detect sorting tracked paths as whole strings,
reordering unscoped worktree reads, and using component order for literal
pathspecs. The symbols corpus stays byte-identical after sharing the comparator.
The cases use canonical slash-separated paths. The competing read errors use
Unix fixtures with a regular file where a parent directory would be needed.

On macOS, all 157 tracked projection cases, 136 worktree cases, and 51
inventory cases matched. The Git unit suite passed 122 tests, the two
changed-target integration suites passed 37, the core target suite passed 52,
and the CI workflow suite passed 222. Guarded Lean checks peaked at
1,225,744 KiB under a 2 GiB limit and a 20-second timeout. Single-job Rust
checks peaked at 2,307,712 KiB under a 3 GiB limit, including the initial
reproduction.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-git-tracked-rows corpus/git-tracked-rows.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-git-worktree corpus/git-worktree.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-git-inventory corpus/git-inventory.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-symbol-resolution corpus/symbol-resolution.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --sensitivity)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope --test lean_changed_target_oracle
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test target_policy
```

## Git patch section transport

Each `diff --git` marker begins a separate file section. The first old header
and the first subsequent new header define its source and destination. A hunk
before that pair produces no range. After the pair, source text that resembles
a path header cannot alter the destination. A nonempty hunk uses the new-side
coordinates, defaults an omitted count to one, and rejects an inclusive end
outside `u32`. Zero-count hunks emit no range. The parser records an exclusion
when a Python source is in scope and the destination is absent or outside the
scope. Later processing applies those exclusions and normalizes the ranges.

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| File-section placement | Prelude and independent lists of classified lines | Renderer places `diff --git` boundaries in bytes passed to `parse_diff` |
| Path validation and scope | Typed header form, canonical relative path, optional selection | Real quote decoding and `GitPathScope` |
| Destination coordinates | Decimal natural or malformed field, optional new count | `parse_hunk_range` reads the rendered hunk header |
| Complete result | Raw ranges in encounter order, excluded path set, or first diagnostic | Real parser maps and `TargetError::GitFailed` at the owned seam |

The 108 `internal-fixture` cases include 80 combinations of four old headers,
four new headers, and five scopes. The 28 additional cases exercise section
isolation, preamble, numeric limits, encoding, duplicate destinations, and
diagnostic order. On an error the caller discards the accumulated maps, so the
corpus compares the diagnostic in that branch. The model treats path spellings
as classified inputs and exercises representative wire encodings. The complete
byte grammar, platform case aliases, and binary numstat are outside this audit.

Lean proves that sections compose in order, concatenating effects on success
and returning the first diagnostic on failure. It also proves that path-header
lookalikes inserted into a body have no effect and that the old coordinate
cannot change the new-side range. A successful range has a positive count and
the corresponding in-bounds inclusive endpoint.
Six sensitivity witnesses detect using the old coordinates, retaining zero-count
hunks, accepting a body header as a destination, merging file sections, ignoring
scope, and omitting exclusions.

Rust matched all 95 success cases and 13 failure cases, which span nine
diagnostics. On macOS, 111 Git unit tests and 30 integration tests passed.
Final Lean checks ran serially under a 20-second deadline and 2 GiB RSS
ceiling, peaking at 1,281,920 KiB. The single-job Rust checks peaked at
2,257,168 KiB under a 3 GiB ceiling.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-git-patch corpus/git-patch.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope --test lean_changed_target_oracle
```

## Quoted Git patch paths

Each named escape supplies one byte. Three octal digits supply one byte when
their value is at most 255. The decoder validates the assembled bytes as
UTF-8. Missing quotes are reported before escape decoding, and a malformed
escape is reported before UTF-8 validation.

Lean generates the input spelling and expected observation from typed text,
named escapes, and octal values. It proves that the generated octal digits
reconstruct the original value, that a successful octal conversion preserves
that value exactly, and that a malformed chunk's error survives any successful
prefix. The model also proves that a missing closing quote takes precedence
over errors in the body.

The 547 cases include all 512 three-digit octal values, all nine named escapes,
and 26 cases for mixed chunks, malformed framing, UTF-8 boundaries, and competing
errors. The Rust adapter calls `decode_git_quoted` and compares either the exact
decoded text or the exact diagnostic. Five sensitivity checks detect retaining
named escape letters, wrapping octal overflow, validating UTF-8 byte by byte,
decoding before checking framing, and accepting a missing closing quote.

The scoped integration test checks that Git emits the filename `unicode-雪.py`
with octal escapes and that Rust returns that filename in changed targets. The
Lean cases concern the quoted-path decoder. Patch section selection and path
scope are covered by the patch transport audit above. The model uses Lean's
UTF-8 validator and compares selected malformed byte sequences against Rust's
validator.

Rust matched all 145 decoded strings and 402 errors. On macOS, 121 Git unit
tests, 37 integration tests, and 222 CI workflow tests passed. The longest
quoted input was 21 bytes. Guarded Lean checks peaked at 1,309,136 KiB under a
2 GiB limit and a 20-second timeout. Single-job Rust checks peaked at
2,266,368 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-git-quote corpus/git-quote.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope --test lean_changed_target_oracle
```

## Git pipe capture

Git stdout returns the complete byte stream when it fits the retention limit.
The first over-limit read returns a typed error. Git stderr keeps the prefix,
continues reading to EOF, then appends one marker if extra bytes were read.
A pipe I/O error ends capture even after the stderr retention limit has been
reached. The capture limit bounds the requested buffer capacity for stdout and
the buffer capacity excluding the fixed marker for stderr.

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Pipe reads | Ordered nonempty byte blocks, injected I/O errors, and implicit EOF | Scripted `AsyncRead` passed to the production `read_pipe` |
| Retention | Complete bytes before the first error, with stderr retention evaluated at EOF | Exact output bytes or typed `CaptureError` |
| Drain | Number of scripted events entered, including an error or EOF | Observed entries at the read boundary, independent of further subdivisions |
| Allocation | Arithmetic bound on the requested growth | Successful `Vec` capacity checked against the configured ceiling |

Lean proves that the stdout and stderr results for complete streams are
independent of the chunk partition. The stderr length is bounded by the
limit plus marker, and an injected read failure is observed after any number
of stderr chunks. For stdout, an I/O error is observed when all preceding bytes
fit, and crossing the cap stops at that read regardless of the remaining
events. When bytes fit the retention ceiling, the growth calculation requests
enough capacity within that ceiling.

The 858 `internal-fixture` cases cover both streams at limits zero through
four over all schedules of up to three steps from four event classes. Eight
named cases exercise the buffer boundary and growth, the 64 KiB stderr cap,
and later I/O errors. Sensitivity checks detect clipping stdout, rejecting an
exact fit, stopping the stderr drain early, retaining an stderr suffix, and
omitting the marker. The existing subprocess tests exercise the 64 MiB stdout
limit, concurrent stderr activity, and cancellation of the owned Git child.

Rust matched 234 successful captures, 237 overflows, and 387 I/O errors on
macOS. The Git unit suite passed 116 tests and the cancellation suite passed
four tests. Guarded Lean checks peaked at 1,242,576 KiB under a 2 GiB limit.
Single-job Rust checks peaked at 2,277,712 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-git-capture corpus/git-capture.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_cancellation
```

## Git shutdown ownership and diagnostics

A failed wait leaves its Git child owned. Cleanup attempts termination once,
records the first wait failure, and waits again. Shutdown may exhaust its
waiting budget while cleanup is pending. A successful wait releases the
owner and keeps the diagnostics recorded before that point. If the cleanup
task disappears before a successful wait, its owner records missing lifecycle
evidence. Shutdown succeeds when every owner is released and the diagnostic
history is empty.

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Independent cleanup owners | One or two child identities with initial I/O and termination outcomes | Actual `ActiveLifecycle` instances and `terminate_and_reap` tasks |
| Wait failures and success | Ordered events carrying the child's result | Controlled `GitChild` replies with Tokio time paused |
| Missing owner evidence | Abandonment event | Abort and join the cleanup task to run `ActiveLifecycle::drop` |
| Shutdown observation | Active count, sticky diagnostics, and budget expiry if owners remain | Actual `drain_for_shutdown` with a zero budget after each event |
| Cleanup return | Termination and first wait diagnostics for the completed owner | Actual cleanup task result in addition to the shared history |

Lean proves that wait failure preserves the active count, subsequent wait
failures retain the first report, and recorded failures persist over arbitrary
event sequences. With a zero wait budget, a successful drain requires an empty
history and zero active owners.

The 313 `internal-fixture` cases include 32 single-owner schedules, 280
interleavings of two owner schedules, and a separate successful two-owner
case. Every case compares intermediate state, including the state after
polling shutdown. Sensitivity checks detect early owner release, duplicate
wait reporting, clearing the history when the last owner exits, suppressing
the missing-evidence diagnostic, and ignoring a remaining owner at expiry.
The subprocess cancellation suite supplies real signal, process, and pipe
observations alongside these scripted cleanup outcomes.

All 2,356 observations matched Rust on macOS. The Git unit suite passed 117
tests and the cancellation suite passed four. Guarded Lean checks peaked at
1,273,024 KiB under a 2 GiB limit. Single-job Rust checks peaked at
2,289,040 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-git-lifecycle corpus/git-lifecycle.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_cancellation
```

## Git spawn admission and cancellation

A standalone Git runner checks its lifecycle cancellation token before
attempting a spawn. A run-scoped runner also checks the request's start gate
and requires the current time to precede its deadline. Lean proves that
denial persists through any sequence of events and that the deadline itself
is outside the admission window.

The 170 schedules cover the standalone and run-scoped policies. Each starts
with up to three events drawn from closing the request gate, cancelling the
lifecycle token, advancing virtual time, and attempting a spawn. Each ends
with a spawn attempt. Rust uses a deliberately missing executable to
distinguish refusal from reaching the operating system. It compares all 284
attempts to the Lean outcomes and checks that each attempt leaves no active
owner or shutdown diagnostic and preserves the cancellation token. Four
sensitivity witnesses cover ignoring either stop signal, admitting a start
at the deadline, and applying the request gate to a standalone runner.

Separate Unix tests drop a waiter before the owner task is polled and close
the request gate while a child is already running. The latter test observes
that the closed gate refuses a second spawn, that the first child stays
alive until lifecycle cancellation, and that the first waiter returns after
the child is reaped. The Lean model concerns admission decisions at ordered
observation points. The subprocess tests exercise the two specified handoffs.

All 284 spawn attempts matched on macOS. The Git unit suite passed 120 tests,
the cancellation integration suite passed four, and the CI workflow tests
passed 222. Guarded Lean checks peaked at 1,245,920 KiB under a 2 GiB limit.
Single-job Rust checks peaked at 2,326,528 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-git-admission corpus/git-admission.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_cancellation
```

## Git resolution command sequence

Changed-target resolution first checks that Git reports a worktree and verifies
an explicit revision when supplied. An exact `show-ref --verify --hash -- HEAD`
lookup then supplies the HEAD object ID, which Hoimin verifies as a commit.
After status 128 from the exact lookup, status 1 from its quiet variant and
a successful symbolic check establish an unborn HEAD. An unborn repository
uses the index for ordinary `--changed` and fails the explicit-base comparison.
Other command failures surface the relevant Git diagnostic.
[Git rev-parse implementation](https://github.com/git/git/blob/v2.39.0/builtin/rev-parse.c#L519-L525),
[Git symbolic-ref implementation](https://github.com/git/git/blob/v2.39.0/builtin/symbolic-ref.c#L13-L35),
[Git show-ref implementation](https://github.com/git/git/blob/v2.39.0/builtin/show-ref.c#L182-L197)

For an explicit base, `merge-base --all` uses the two resolved commit IDs and
must yield one common ancestor. The inventory, patch, and numstat commands
all take the same resolved comparison ID and a revision/path separator.
A successful resolution ends by reading untracked files.
[Git merge-base options](https://git-scm.com/docs/git-merge-base#_options),
[Git's diff merge-base checks](https://github.com/git/git/blob/v2.43.0/diff-lib.c#L527-L564)

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Repository query and comparison basis | Worktree reply, optional revision, HEAD status, and commit ID bytes | Fixture responses returned to the normal subprocess runner |
| Tracked path selection | Scope and typed inventory, using `GitInventory.plan` | Real scoped planner and patch/numstat decoders |
| Request sequence | Ordered argv vectors and replies through the first semantic failure | Exact NUL-delimited argv captured by the fixture Git executable |
| Final result | Physical range map or failure class | Actual result of `resolve_changed_scoped` against temporary source files |

The 55 `internal-fixture` cases use a subprocess-local fixture Git and run on
Unix. They cover tracked, explicit-base, and indexed routes over four scopes,
plus revision validation, rename fallback, literal pathspec flags, and failure
at individual requests. Commit ID validation distinguishes unsuccessful
commands, malformed UTF-8, and invalid hexadecimal form and accepts 40- and
64-byte hexadecimal IDs. The fixture supplies distinct IDs for the raw HEAD
object, the peeled HEAD commit, the requested base, and the merge base. The
corpus checks missing and multiple merge bases, failure during HEAD lookup
and verification, and signal termination. Git supplies the graph and rename
outcomes for the command dispatcher.

Lean proves that an explicit revision appears directly after the worktree
check in the planned sequence and that execution stops at the first failure
for every command list, including when merge-base resolution fails. After the
exact HEAD lookup returns 128, the indexed
request occurs exactly when the quiet exact lookup returns 1 and the symbolic
check succeeds. Nine sensitivity checks detect substituting HEAD for
an explicit base, bypassing the inventory, continuing after an error,
attempting a patch when HEAD is absent, treating a fatal HEAD probe as a
missing commit, indexing without confirming an unborn HEAD, using the raw
HEAD object or requested base directly as a comparison, and omitting
untracked discovery.

### A tag named HEAD

In a temporary repository with a staged three-line file, a symbolic HEAD aimed
at an unborn branch produced three candidates. Adding `refs/tags/HEAD` with
the seed commit caused the earlier resolver to pick that commit and return
zero candidates. An explicit base also succeeded in that state. The real-Git
regressions check the repaired three-line result and require an explicit-base
error. Separate commit graphs with no common ancestor and with two best
common ancestors exercise the old and new diagnostic contract.

All 55 generated cases matched Rust on macOS. The Git unit suite passed 115
tests and the integration suites passed 84. Lean ran serially under a
20-second deadline and 2 GiB RSS limit. The single-job Rust checks peaked at
2,421,856 KiB under a 3 GiB limit.

### Corrupt HEAD counterexample

A real-Git regression corrupts a loose commit object in a temporary repository.
Git still confirms the worktree and then exits 128 when it tries to unpack HEAD.
Before the repair, the control selected only changed line 2, and the corrupt
case succeeded with lines 1, 2, and 3. The regression reproduced that range
expansion before the implementation change.

After the repair, target resolution reports the failing object lookup or
commit verification command and its stderr. The same reproduction exits at
target resolution with CLI status 2.

Removing the loose object instead of corrupting its bytes made the commit
probe return 1. The regression reproduced the same expansion to lines 1–3
on both a symbolic branch and a detached HEAD. The repaired cases report
the missing object from the exact `show-ref` check. The unborn repository
fixtures still read staged and untracked Python sources.

Manual CLI replay with Git 2.56.1 confirmed status 0 and all three staged
lines for an unborn branch. A missing object, a malformed branch ref, and
a branch ref pointing at a blob each returned CLI status 2 with empty stdout
and a Git diagnostic.

Reproduce in an isolated temporary repository with a built debug CLI.

```python
import json
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory

hoimin = Path("target/debug/hoimin").resolve()
with TemporaryDirectory() as directory:
    root = Path(directory)

    def git(*args):
        return subprocess.run(["git", *args], cwd=root, capture_output=True)

    source = root / "app.py"
    git("init", "-q").check_returncode()
    source.write_text("a = 1 + 2\nb = 3 + 4\nc = 5 + 6\n")
    git("add", "app.py").check_returncode()
    git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
        "-c", "commit.gpgsign=false", "commit", "-qm", "fixture").check_returncode()
    source.write_text("a = 1 + 2\nb = 3 + 9\nc = 5 + 6\n")

    def plan():
        result = subprocess.run(
            [hoimin, "plan", "--root", root, "--source", ".", "--changed",
             "--operators", "binary_add_sub", "--allow-best-effort-memory",
             "--", sys.executable, "-c", "pass"],
            cwd=root, capture_output=True, text=True,
        )
        return result.returncode, (
            [case["line"] for case in json.loads(result.stdout)["candidates"]]
            if result.returncode == 0 else result.stderr
        )

    print("control", plan())
    oid = git("rev-parse", "HEAD").stdout.decode().strip()
    obj = root / ".git" / "objects" / oid[:2] / oid[2:]
    obj.unlink()
    obj.write_bytes(b"not a git object")
    print("probe", git("rev-parse", "--verify", "--quiet", "HEAD^{commit}").returncode)
    print("corrupt", plan())
```

```sh
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --check-git-resolution corpus/git-resolution.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_lines -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope --test changed_git_environment --test changed_git_cancellation --test changed_excluded_paths --test lean_changed_target_oracle
```

## Binary numstat exclusions

A numstat record is binary when both count fields are dashes. A regular binary
record contributes its one path. A binary rename contributes both paths, each
checked independently for Python extension and scope membership. For any rename,
the parser consumes both NUL-delimited paths even when the counts describe
text. Binary paths are decoded as UTF-8 before scope filtering. Literal
backslashes are rejected in unscoped reads and ignored in scoped reads.

| Premise or observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Count fields and framing | Count status and typed regular, rename, padding, or malformed record | Corpus renderer delivers exact `--numstat -z` bytes, including invalid UTF-8 |
| Scope and preexisting exclusions | Canonical optional path scope and seed paths | Real `GitPathScope` and the set passed to `parse_binary_numstat` |
| Final tracked ranges | Membership difference over already normalized patch ranges | `GitChangedRanges::from_patch`, called by tracked collection |
| Rejection | First structural or path diagnostic | Exact `TargetError::GitFailed` message |

The 419 `internal-fixture` cases include 400 combinations of count fields,
regular and rename path forms, and five scopes. Nineteen additional cases
exercise NUL framing, padding, duplicates, unusual path spellings, ignored text
path encodings, seeded exclusions, and competing errors. A failed parse has no
final tracked range result. Git's binary classification, platform case aliases,
and the subsequent conversion into Python physical rows stay outside this
numstat model. Real Git integration tests exercise the call through public
target resolution.

Lean proves that a binary rename contributes exactly the selected Python
endpoints, that replay of a successful stream preserves exclusion membership,
and that an excluded path cannot survive the final filter. The six sensitivity
witnesses detect using the new name alone, treating one dash as binary,
ignoring malformed text-rename framing, bypassing scope, forgetting exclusions
already recorded by the patch, and skipping the final filter.

Rust matched all 402 success cases and 17 error cases. On macOS, 112 Git
unit tests and 30 integration tests passed. Lean ran serially with a 20-second
timeout and a 2 GiB process-tree RSS limit, peaking at 1,333,424 KiB. The
single-job Rust checks peaked at 2,296,320 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_changed_context -- --check-git-numstat corpus/git-numstat.jsonl)
(cd formal/HoiminOracle && lake exe generate_changed_context -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib target::git
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test changed_git_scope --test lean_changed_target_oracle
```
