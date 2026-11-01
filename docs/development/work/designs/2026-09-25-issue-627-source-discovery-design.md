# Issue 627: scope source discovery

Source selection currently disables the existing exact-path walk filter, so files
below unrelated directories are visited and retained until core resolution. Extend
the same root-started walk with source-subtree membership. Preserve root-relative
ignore/include/exclude evaluation, symlink/default exclusions, path normalization,
platform case rules, source order for symbol resolution, and exact/line selectors.

The scope contains normalized exact files, their directory ancestors, normalized
source roots, and source ancestors. Keep a directory if it is an ancestor needed
to reach a requested path or lies in a source subtree. Keep a file if requested
exactly or inside a source subtree. Membership walks the entry's component prefixes
against indexed roots, avoiding a scan of every source for every entry. Source
roots remain in their original order in Selection; the index only prunes discovery.
Symbol-plus-source lookup stays inside those roots. Symbol-only or selector-free
discovery remains broad. A normalized root source (empty or dot) also stays broad.
Invalid normalized selections fall back to existing discovery/validation behavior.

Continue walking from the project root; starting a walker at each source would
change inherited ignore and include restoration. Keep non-Python inventory entries
inside selected roots because this change concerns scope, not file-kind policy.
Keep raw/native ancestry handling so invalid names inside a selected source still
reach the existing portable-path diagnostic, including Windows case matching.

The diagnostic boundary matches the previous exact-selector optimization: errors
on the root and selected traversal remain visible; malformed filenames and walk
errors solely in pruned subtrees are outside the request and no longer diagnosed.
Root-level sibling enumeration may remain. Includes cannot select a mutation path
outside the source scope; excluded/default-protected paths remain excluded.
Explicit source existence checking belongs to Issue 619 and is not changed here.

Actual walker counters establish that adding 0/1000/5000 unrelated files does not
increase visited descendant entries or collected records. Compare resolved targets
with an actual broad-discovery control for mixed/normalized/ignored/symbol cases,
and compare complete candidate arrays through public plan with a dry-run marker.
No time, RSS, filesystem-call, or global constant-time guarantee is claimed.

## Design reviews

1. Read core resolve_explicit and resolve_symbol_path. Symbols iterate original
   source order and do not search outside source roots, so source-scoped discovery
   is safe even with symbols; retain the original Selection unchanged.
2. Read both root-started walkers and existing native fallback. Filtering only
   collected files would not reduce traversal; apply scope to normal and include
   walks. Native malformed descendants require ancestor matching to preserve
   selected-source errors rather than silently pruning malformed names.
3. Checked root-equal, regular-file sources, overlapping roots, and exact paths
   outside sources. Root means broad discovery; a file source selects itself;
   exact/line roots are included for prior error precedence. Record the intentional
   out-of-scope diagnostic boundary and preserve files' existing type policy.
