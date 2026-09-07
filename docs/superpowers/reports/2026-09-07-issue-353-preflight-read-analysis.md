# Issue #353: preflight read-count analysis

## Current behavior

`WorkspacePlan::preflight_validated_in` performs three content-read phases:

1. Build a complete manifest, check owned-workspace capacity, and run the validation callback.
2. Read each manifest file again, verify its size and hash, and write the shared snapshot.
3. Walk the source tree again and compare a fresh path/size/hash manifest before returning the plan.

The validation callback runs before snapshot allocation or copying. The final
walk also detects additions and removals and samples source content after the
snapshot copy.

## Evaluated optimization

A combined second pass could build the fresh manifest and copy each file from
the same bytes used for hashing. It would preserve exact snapshot-to-manifest
content agreement while reducing ordinary source reads from three to two.
It would change the timing of source-change detection.

For example, the first pass records file `a.py` as content A. A combined pass
reads and copies A, then another writer changes `a.py` to same-size content B
before that pass finishes. The combined pass can succeed. The current separate
third pass would detect B if it reads the file after that change. Both designs
allow changes after their last observation; neither provides an atomic
filesystem snapshot. The optimization nevertheless removes the separate
post-copy content observation.

## Decision

Retain the existing implementation. A two-read design needs a separate contract
decision about post-copy source verification, or permission to retain the first
pass's bytes before validation. Retaining an entire project in RAM introduces
an unbounded memory cost; spooling it to disk moves copying before validation.
A metadata-only final walk cannot detect same-size, same-mtime content changes.

The analysis also identified constraints for any later implementation:

- Preserve capacity and validation failure behavior before snapshot allocation.
- Treat repeated visits to the same logical path from include walks separately
  from distinct paths that alias on the destination filesystem.
- Keep partial snapshots under their pending owner until the final manifest
  comparison succeeds, including managed-workspace cleanup on failure.
- Measure content reads and hashes across both manifest and copy code paths.

No production optimization is included with this report.
