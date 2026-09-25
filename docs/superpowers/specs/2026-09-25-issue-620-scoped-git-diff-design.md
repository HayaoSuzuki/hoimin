# Scoped tracked Git diff design

## Problem and acceptance

Issue 620 measures plan time and Hoimin memory growing with the body of an unrelated tracked CSV change even when exactly one Python file is selected. Avoid generating or collecting those unrelated patch bodies for the ordinary tracked-file case. Preserve changed target identity/ranges across deletion, binary exclusion, untracked/unborn, diff-base, rename, changed-context and 612's Python physical rows. Selection/name validation remains consistent with 621.

## Alternatives and decision

A pathspec alone changes rename detection: a probe on Git showed a renamed selected destination becoming a 31-line addition instead of the original one-line rename modification. Streaming the existing global patch would reduce retained memory but still generates and transports unrelated bodies. Computing replacement blob diffs ourselves would need to reproduce Git clean filters, attributes and rename pairing.

Use an initial global `git diff --name-status -z` inventory with the same base/merge-base and rename options. This output contains status and path metadata, never patch bodies. Decode each raw path, determine membership using the existing GitPathScope, then apply portable validation; retain current structural/UTF-8 failure handling. For eligible non-rename changes, run patch and numstat with literal pathspecs and explicit `--no-renames`. Disabling scoped rename detection prevents a subset from creating a pairing that the global inventory did not choose. Paths come from Git's actual spelling, preserving platform equality without inventing path spellings.

If any eligible rename/copy destination appears, retain the current global patch/numstat flow. This deliberate performance fallback preserves exact global pairing and hunk semantics without reproducing Git's content-conversion pipeline. Unrelated rename/copy destinations do not force fallback. A path too large for the bounded argument policy also uses this safe fallback. The unscoped public Git API keeps its existing flow.

Batch scoped paths by both count (128 maximum) and total UTF-8 path bytes (8192 maximum), with sorted/deduplicated inventory destinations. Invoke Git with `--literal-pathspecs` before the diff subcommand and `--` before paths, protecting wildcard/magic/option-looking names. Fixed options add only bounded overhead. Empty eligible results skip patch/numstat after the inventory has validated repository/base; invalid diff-base and non-repository errors remain observable. Indexed-unborn and untracked collection remain unchanged.

The metadata inventory still covers the repository and rename detection can still inspect unrelated content; this is not a claim of constant cost for arbitrary repository size or rename workloads. The common modified-file case no longer scales in patch output/memory with unrelated modified contents. Relevant rename/copy cases retain the old cost and are explicitly documented.

## Validation and empirical evidence

Before code, preserve a release binary and run the original issue measurement script: 4/16 MiB CSV, clean/dirty/clean-repeat, changed/plain, three repetitions (36 plans). Retain sampled Hoimin-only RSS separately from wait4 child-inclusive RSS. Repeat after changes with the same script and assert candidate identity consistency across phases.

A public CLI regression uses a subprocess-local Git wrapper that invokes real Git, forwards stdout/stderr unchanged and records actual output byte counts. Two fixed-file-count fixture sizes must produce the same candidate and bounded patch/numstat bytes independent of excluded CSV body size. This directly detects the old ingestion behavior without timing/RSS thresholds. Wrapper output capture is instrumentation only; empirical timings use unwrapped release binaries. Additional public target/CLI tests compare scoped results against the unchanged unscoped resolver plus intersection, including relevant/unrelated/into/out-of-scope renames, binary and deletion, diff-base, changed-context, unborn/untracked, literal names and multiple batches.

No new Lean predicate model is useful for Git's external rename implementation or syscall output cost. Existing changed-target/context/lines Lean-generated public adapters remain the correspondence regression suite. Direct Git observations and measured subprocess output provide the independent oracle for this change. No Lean command or lease is required unless a later change actually affects a model.

## Design reviews before code

1. Rename correctness: pathspecs change the set competing for rename sources, so merely including the selected destination is unsound. Inventory first, disable rename detection for known non-rename outputs, and retain exact legacy flow whenever a selected rename/copy exists. Root independently agreed with this conservative fallback.
2. Path and bounds: Git paths can contain glob/magic syntax and many long paths can exceed argv limits. Use actual decoded Git spelling, literal mode plus separator, and count+byte bounds. Membership stays ahead of portable validation, preserving excluded literal-backslash behavior from 621.
3. Error and performance scope: empty selection must not bypass bad base/repository validation. Metadata still runs and untracked/unborn behavior stays intact. Report the rename and oversized-path fallbacks honestly; measure Hoimin RSS separately from child usage and avoid claiming full-repository work disappears.
