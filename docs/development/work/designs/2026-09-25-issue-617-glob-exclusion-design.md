# Issue 617: one exclusion language at the discovery boundary

Filesystem discovery already applies include/exclude globs, ignore rules and built-in copy exclusions. The pure core resolver mistakenly applies the exclude strings a second time as literal paths. Remove that second predicate and make the resolver's contract explicit: its discovered inventory must already reflect the discovery policy. Do not add a second glob implementation or a filesystem dependency to core.

`TargetHandler` already passes precisely that filtered inventory. Audit the other production callers and update the direct core test to provide prefiltered input. Keep missing/non-Python explicit-path errors when discovery actually excludes a requested path. Source/file/line selections must retain bracket-named files not matched by the glob, and escaped bracket globs must continue to exclude their literal target. Preserve include precedence and fixed exclusions by leaving discovery unchanged.

Promote the existing finite 20-case Lean audit: four patterns, five files, source/file/line selectors. Its filter-idempotence theorem and broken literal-comparison controls describe this boundary, not generic glob parsing. Compare real discovery, core resolution and public plan against generated expectations, with a public run regression for the bracket-named file.

## Design self-review

1. Responsibility: inspected discovery's two walks and OverrideBuilder, then core's raw-string equality filter. Removing only the latter preserves the sole semantic glob implementation and avoids introducing ignore dependencies into core.
2. Public API: `resolve_explicit` is publicly callable and a direct test assumes literal filtering. Document prefiltered-inventory responsibility and update that test explicitly; retain pipeline tests for ordinary exclusion precedence instead of silently weakening that contract.
3. Boundaries: preserve path normalization, Python detection, explicit error classification and set/range normalization. The finite escape semantics were audited on Unix; distinguish that correspondence from Windows glob behavior and retain portable core coverage.
