# Issue 484: Reject metrics destinations that replace protected input entries

Issue: https://github.com/tokyogas-tech/hoimin/issues/484

## Failure and required behavior

Metrics output is finalized after original-source verification. NamedTempFile::persist replaces its destination entry, so selecting calc.py as both mutation input and metrics output can finish successfully and then replace original code with JSON. A session DB entry can be replaced as well. Reject a collision with selected source files, explicit fingerprint inputs and the active session database before the baseline starts. The diagnostic must identify the destination and protected input. Original bytes must remain unchanged, and the collision-failure finalizer must have no permission to perform that write.

Keep normal metrics output and updating an existing metrics file, including outputs inside the project. Preserve metrics for baseline failure and incomplete modeled runs after output collision validation. Existing unusable-output behavior is also contractual: metrics pointing to an ordinary directory warns metrics.write after the run and does not change run status/counts. This issue adds collision protection, not a general fatal validation of output writability.

## Destination identity

Model the entry replaced by rename: resolved parent directory plus the destination's own filename, with actual filesystem aliases and case behavior accounted for. Do not compare all paths only by final inode or blindly canonicalize the final symlink. A distinct hardlink entry can be replaced without writing through to its protected sibling; a separate metrics symlink can likewise be replaced without modifying its target. Parent symlinks, absolute/CWD-relative spellings, dot/dotdot and case aliases that reach the same entry must collide.

Use existing path/filesystem facilities where possible, with a narrowly scoped entry helper if necessary. Do not lower-case every path on every platform or assume all Windows filesystems have identical case behavior. Test native supported cases and report unexecuted platform branches accurately. The local macOS realpath diagnostic normalizes regular-file case and preserves distinct hardlink filenames, but follows a final symlink; it is evidence for that environment, not a complete cross-platform algorithm.

For sessions, protect the configured DB entry and the actual opened database entry where aliases make them differ; replacing the configured access path can break subsequent resume even if a referent inode survives. Preserve the safe case of a separate metrics link pointing to a protected DB. Inspect SQLite path resolution and sidecar behavior instead of relying on WAL recovery for safety. Include the exact active SQLite -wal/-shm/-journal entries and canonical .<database-name>.hoimin-locks ownership tree when establishing protected session artifacts; replacing an ownership lock entry must not create a second lock identity. Derive this literal active namespace from the existing session implementation, without broad filename-prefix exclusions. This independent branch starts from 4adf809 and does not assume #472 SessionArtifacts exists.

## Preflight and finalization ownership

Resolved targets and configuration are available at the existing owned blocking Preflight effect. Perform filesystem collision inspection there, before baseline and without new async-task blocking IO or constructor filesystem requirements. Retain workspace ownership through success and failure. A small typed validated destination or explicit collision authorization can travel with the existing owned completion into ShellContext before the core event is applied; prefer that bounded shape over new core phases or generic capability frameworks.

The finalizer currently writes on run_result Ok, including modeled nonzero results. An EffectFailed collision event alone is therefore insufficient: the raw configured metrics path must not grant write permission after rejection. Derive final output permission from the completed collision check and carry the resolved destination entry to the final writer. Do not validate a resolved parent/entry and then write through the original aliased configuration path. This does not introduce a claim of protection against arbitrary concurrent filesystem replacement. Preserve ordinary failure metrics after authorization even if later preflight/baseline work fails, and retain shutdown deadline, owned blocking finalization and warning semantics. Inspect earlier target/preflight failure behavior before changing it; identify concrete existing guarantees rather than assuming all failures are identical.

If destination inspection finds an unrelated inability to write (directory, missing parent, permission), preserve the existing late warning contract without permitting a protected overwrite. Establish a safe distinction between a confirmed collision and unusable output. Do not silently swallow an identity check failure and then authorize an unknown potentially colliding entry. Report a concrete design refinement if this distinction requires changing a previously observed failure path.

## Verification

Use temporary projects only for destructive RED, capturing actual baseline success and original-source replacement on controlled old code. After correction, actual CLI run collisions return an explicit failure before a Python baseline marker is created. Assert source/fingerprint/session bytes and hashes remain equal, session DB header remains SQLite and a subsequent connection/resume remains usable as appropriate. Do not infer safety from WAL repair.

Cover selected source and explicit fingerprint input, new/existing session DB, absolute and CWD-relative metrics spellings, dot/dotdot, parent-directory alias and supported case aliases. Pair each with noncolliding controls: ordinary in-root output, existing metrics update, distinct hardlink and final-symlink entries, and baseline failure metrics. Verify that replacing the distinct link removes only that entry and retains protected bytes. Preserve source candidate IDs/spans and run outcomes in controls. Exercise the collision finalizer path, normal cleanup and deadline behavior, not just a standalone comparator.

Read existing output/finalization state-oracle tests; Lean is useful only for a concrete new permission transition with real implementation correspondence. Path identity and filesystem rename semantics require native observations and are not established by abstract string equality. No formal proof may substitute for actual public CLI/byte-preservation regressions.

Run focused metrics/public/shell lifecycle tests, the full workspace with all features, fmt and Clippy for all targets and features. User worktree files are outside all destructive fixtures. No runtime Python injection or project-wide blanket output ban.

## Design self-review

1. Traced writer persist and outer run finalization after source verification. Distinguished Rust Err from modeled Ok(nonzero): baseline-before-rejection plus a closed finalizer write gate are both required.
2. Compared rename entry identity with referent/inode identity and native macOS realpath probe. Included safe distinct link controls, parentaliases and session configured/actualentry distinction; avoided unproved cross-platform string folding.
3. Read complete/failedbaseline/directory-output metrics tests and owned blocking Preflight handoff. Preserve late warning and failure metrics contracts, cleanup ownership and shutdown budget; no constructor-I/O or arbitrary capability refactor.

## Ruling: prospective entries and unresolved identity

Session opening is lazy, so a new DB and its future sidecar/lock entries may both be absent during preflight. Existing-leaf canonicalization cannot identify prospective case aliases. Retain the session lifecycle and use a narrow native query of the parent directory's case behavior. Compare parents first. Existing regular leaves preserve native entry spelling, including distinct hardlinks; a final symlink retains its own entry, using exact spelling or an unambiguous no-follow entry identity. Prospective ASCII names use the known parent sensitivity. Do not claim complete Unicode or filesystem alias modeling.

If identity cannot be established, carry a typed withheld destination and never give the finalizer permission to write it. Keep the run result and use the existing late metrics.write warning. Known collisions still fail before baseline; unresolved identity withholds metrics instead. This differs from claiming that every possible collision is recognized before baseline.

Controller chose this bounded approach instead of moving session creation/migration earlier or creating a probe at the user-visible output path. Cost: additional platform-specific directory queries and conservative omission of metrics for an otherwise writable ambiguous or unsupported destination. Record this ruling and cost in the final user report. Native new-DB case-alias tests must show rejection before baseline, while ordinary outputs and the withheld/no-write path remain covered.

## Resolved implementation details

Unix inspection caches directory entry names once per parent directory identity. Exact names remain distinct even when three or more hardlinks share an inode; a lazy no-follow alias index resolves only unambiguous inexact spellings. File inode equality never defines protected replacement-entry equality. Native prospective case queries are restricted to their documented scope: macOS directory pathconf, Windows case-sensitive directory information, and Linux ext4/F2FS casefold flags with an int ioctl output. Unsupported identity remains withheld.

The bundled SQLite VFS resolves final DB symlinks on Unix; Windows GetFullPathNameW retains the configured final entry for sidecar naming. Protect both database access entries, the OS-specific sidecar namespace, and both the literal and resolved ownership tree when that directory is a symlink. Do not let unrelated session uncertainty hide an already confirmed source collision.

An absent destination parent cannot grant permission to write later: after checking confirmed collisions, including future ownership entries, require the parent to resolve during preflight. Otherwise retain the withheld state. This prevents a baseline-created parent symlink from changing a prospectively synthesized destination; no arbitrary concurrent replacement guarantee is added.

Preserve path syntax that constrains the original rename operation. Directory-only terminal syntax (a separator, `/.`, or `/..`, including native Windows separator spellings) must retain the existing late warning. Inspect the raw terminal component before normalization; dropping `/.` or a separator could wrongly authorize replacement of a directory symlink entry. A regular file followed by a separator likewise remains unusable. Native public regressions cover these spellings.
