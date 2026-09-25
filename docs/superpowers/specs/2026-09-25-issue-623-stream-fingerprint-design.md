# Issue 623: bounded auxiliary-input hashing

Auxiliary fingerprint inputs need only a BLAKE3 digest. Replace their full Vec reads with the locked BLAKE3 implementation's streaming reader (fixed 64-KiB buffer). Preserve sorted records, exact-input evaluation order and duplicate read behavior, glob-before-exact error precedence, and final digest encoding. No schema or hash-format change is needed.

Reuse the existing WorkerRoot capability boundary: open and validate a regular non-symlink file through the pinned parent, then consume the inspected handle. Share that open path between byte reads and hashing; retain Unix O_NONBLOCK and post-open metadata checks, Windows final-handle checks, and existing missing-versus-unsupported mapping. Do not reopen by a concatenated path. Existing read consumers retain their Vec behavior.

## Design review passes

1. Safety: inspected WorkerRoot::read on both platforms and RootRelativeReader's missing classifier. Factoring only the consumption step keeps parent pinning, non-file rejection and read-error labels; the Windows error reclassification must enclose both open and consumption, as before.
2. Semantics: inspected both exact and deferred-glob branches in fingerprint_inputs::resolve. Each exact input is still read in caller order, including duplicate inputs. Only the digest replaces temporary contents. Glob/exact overlap reuses the exact digest as today.
3. Bound: checked locked blake3::Hasher::update_reader and its copy_wide implementation: 64-KiB stack buffer, interrupted-read retry, no mmap or unbounded Vec. Use deterministic allocation measurement and byte-for-byte digest controls, not a new input limit. Lean adds little for this byte-stream implementation equivalence; direct binary fixtures and capability race tests are the relevant checks.
