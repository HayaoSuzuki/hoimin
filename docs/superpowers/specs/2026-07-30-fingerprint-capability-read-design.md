# Fingerprint Capability Read Design

## Scope

Fix #63 so fingerprint files are read without traversing symlinked or reparse
point parent components and without reopening a path after validation.

## Design

Reuse the workspace root's existing cross-platform capability-relative reader.
It opens the root once, walks every parent with no-follow directory handles,
opens the final regular file without following links, and reads bytes from that
validated handle. Expose a narrow crate-private helper from the workspace
module rather than duplicating Unix and Windows security logic.

Fingerprint discovery and exact-path normalization remain responsible for
selection and error categories. The final hashing pass uses the capability
reader for both glob and exact inputs. Existing preflight metadata checks may
improve error specificity, but they are not trusted for the bytes being hashed.

## Error handling

Capability-reader errors are mapped to the existing exact or glob
`unsupported_file` variants. The existing explicit missing-file preflight
continues to produce `fingerprint.file.not_found`.

## Testing

Add a regression test in which an intermediate directory is a symlink to an
outside directory and assert an exact fingerprint input is rejected. The
shared workspace reader already has deterministic parent-replacement tests on
its opened-parent handle; retaining those tests demonstrates that the bytes
come from the validated capability rather than a reopened pathname.
