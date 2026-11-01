# Issue 607: stale plan path diagnostics

Saved-plan verification currently reports that source or fingerprint records differ without identifying the files. Enrich the existing mismatch diagnostic using the records already available to `ensure_exact_records`.

Compare records by root-relative path, preserving the existing map equality semantics. A saved-only path is `removed`, a current-only path is `added`, and a shared path with a different hash is `modified`. Unchanged paths are excluded. Display differences in ascending path order, regardless of input record order or change kind. Show at most ten paths and append `; N additional paths omitted` when necessary. Each path uses Rust string Debug quoting, keeping newlines, tabs, quotes and other control characters inside one diagnostic line. Retain the existing message prefix, followed by `: modified "src/b.py"; ...`.

Both record kinds use the same formatter in shared verification preparation, so normal verification and dry-run retain identical errors, exit 2, empty stdout and rejection before test execution. Source mismatch remains earlier than fingerprint mismatch. Resolution/read failures that occur before record comparison keep their existing diagnostics. No extra filesystem access, hashing or candidate analysis is introduced; selection, manifest and output schemas stay unchanged.

Alternatives considered: grouping by change kind would obscure global path ordering; unbounded details could overwhelm stderr; re-discovery for diagnostics would duplicate work and risk inconsistent snapshots. A fixed ten-path limit and existing record maps make behavior explicit without a new CLI option. A pure comparison needs ordinary table/boundary tests, not a new state model or Lean proof.

## Design self-review

1. Traced normal and preview preparation through source resolution, source hashing, comparison, fingerprint resolution and comparison. The shared function covers both modes without moving validation or masking earlier errors.
2. Checked added/removed orientation against the issue's saved/current definitions and considered empty collections, unchanged entries and reordered inputs. Retaining `record_map` preserves equality semantics and existing duplicate handling.
3. Reviewed output limits and hostile path text. Chose one global lexical ordering, an explicit ten-entry cap and an exact omitted count; Debug string quoting prevents embedded controls from becoming diagnostic separators. No absolute root path or new read is required.
