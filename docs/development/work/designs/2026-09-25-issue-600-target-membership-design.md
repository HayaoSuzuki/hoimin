# Issue #600: Index verify target membership

## Intent and scope

Remove the repeated candidate-by-target scan in `validate_requested_descriptors` without changing accepted candidates, diagnostics, selection, ranking, or persisted schemas. Baseline: `4e3bc2a97cad2e4ce8ae2b11c970fc2f5dff9bb2`. Issue #600 measures 10,000 candidates in one file at the beginning or end of up to 10,000 selected files. Existing issue #456 file-level preprocessing must remain intact.

## Design

Build one borrowed `HashSet<&Utf8Path>` from `targets` before the requested-ID loop. Use `selected_paths.contains(candidate.path.as_path())` at the existing membership-check position. Keep candidate lookup first, membership next, then source read/context creation, then the stored descriptor/stable-ID result. Keep the `BTreeSet<String>` iteration order and per-file result cache exactly as they are. Index iteration order is never used for output or validation order.

Use `Utf8Path` keys rather than strings: camino equality compares components and its hash delegates to the underlying `Path`. Repeated separators, interior dot components, and trailing separators can compare equal; leading `./`, parent components, and case differences retain their existing distinctions. Do not canonicalize, resolve symlinks, fold case, or call selector-specific normalization. Targets may contain duplicate equivalent paths; the set removes duplicates only for membership.

Index construction consumes F borrowed paths and uses O(F) additional entries. C existing requested candidates perform C hash-set queries: expected O(F+C) membership operations, excluding path length and hash collisions. This is not a worst-case hashing bound or a complexity claim for descriptor validation, grouping, rediscovery, workspace copy, or complete verify.

## Alternatives

A `BTreeSet<&Utf8Path>` preserves equality and guarantees logarithmic lookup, but adds comparison work to a membership-only index. Sharing the existing linear scan once per requested path is smaller but still incurs P×F comparisons with P distinct requested paths. Choose the hash set because it removes that scan for concentrated and distributed candidates without owning path strings or changing public APIs.

## Compatibility and validation

Keep plan schema 4 and ranking rule 4. Reuse existing candidate validity, rediscovery, strict/diverse selection and CLI tests. Add a path matrix exercised through descriptor validation, comparing actual target membership with explicit equality expectations (repeated separators/interior dot/trailing slash versus case/leading dot/parent components), including duplicate targets and no targets.

Add invalid requests from multiple files covering unknown ID, unselected path, unreadable source, descriptor failure, and stable-ID failure in both ID orders. Include a valid earlier candidate sharing a file with a later invalid candidate, so batched descriptor validation must not return that later error ahead of another file. Unselected nonexistent files must report membership failure before attempting a read.

Count actual target-iterator visits and membership queries in the test-only `ValidationStats`. Before the change, put the target-visit increment inside the existing `any` predicate; after it, put it inside the iterator constructing the index. Vary C in 1/4/16 and F in 1/8/64 independently, concentrating all candidates in the final target. Assert F visits and C queries on successful runs. This counts target traversal and collection queries, not hash-internal comparisons or allocations; retain a demonstrated failing legacy scan with C×F visits.

Repeat the issue's public preparation API and CLI release measurements using the existing six fixtures, alternating first/last over three repeats per size. Write new outputs outside the historical directory; verify selected count, baseline Exit(1), zero mutants, and exit 3. Timings are observations, not CI thresholds. Record environment and binary hash, build contention, and any unmeasured platforms.

## Design self-review

1. Contract review against `plan.rs:validate_requested_descriptors` and issue #456: rejected pre-validating all requested memberships because it would let a later unselected path outrank an earlier descriptor/read error. Fixed design to change only the existing membership expression.
2. Equality review against camino 1.2.6 `Utf8Path::{eq,hash}`: rejected raw-string keys; explicitly added component-equivalent spellings, leading dot, parent components and case boundaries. No filesystem equivalence is introduced.
3. Cost/evidence review against the issue acceptance criteria: clarified expected hashing complexity, target-visit counter placement, independent C/F variation, and separation of API/CLI observations from CI gates. Added distributed-path rationale and duplicate-target behavior. No implementation or benchmark success is asserted at this stage.
