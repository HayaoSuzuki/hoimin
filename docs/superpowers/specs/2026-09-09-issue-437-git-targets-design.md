# Issue #437: Align Git current-file reads with resolved targets

## Contract

Git discovery must not open symlinks or special files to count changed Python lines. The TargetHandler path must only read current files already selected by filesystem discovery and explicit selectors, including exclusions. The standalone public handle_git API must remain usable without a Selection, while excluding links and nonregular files from current-file reads. Tracked patch parsing, rename/binary handling, commit resolution, missing files, line counts, and public request/response shapes remain compatible.

## Architecture

Add an internal scoped Git resolution entry used by TargetHandler after resolve_explicit. Keep the existing unscoped entry for handle_git. Carry the optional eligible target slices into both indexed-without-HEAD and untracked current-file collection. Filter before file-content I/O through the existing core changed-target index, preserving its platform logical-path equality (including Windows case rules) without scanning every eligible target for each Git path. An explicitly empty eligible set still decodes Git paths and retains repository/revision validation, then performs no current-file reads.

Use the existing WorkerRoot capability reader for eligible current-file reads instead of ambient tokio::fs::read. Its retained directory traversal rejects parent links; a shared final-entry capability check rejects links/reparse points and nonregular files before either platform opens them. If the final entry changes to unsupported after that check, a no-follow read failure is structurally reclassified through the same capability metadata. Expose only the existing crate-private type through workspace, without a new public API or duplicated platform-specific open implementation. Run the synchronous capability reads in a blocking task, not on the async runtime thread.

Unsupported link/nonregular paths and disappeared files contribute no changed lines. Preserve real I/O failures (permissions, read errors) as Git failures. WorkerRoot::read reports InvalidPath for unsupported entries; its is_missing method can distinguish a disappearance after failed reads without string matching on diagnostics. Git path decoding/portable validation remains before the reader, so malformed logical paths retain existing rejection. Avoid retaining a whole batch of file contents: read and count one file at a time.

The safe reader is the boundary even after an eligibility check. A target can change between discovery and read; checking symlink_metadata followed by an ordinary open would reintroduce the failure. The existing reader prevents following replacement links and blocking on replacement FIFOs. Do not claim an immutable project snapshot: regular-file contents can still change, under existing source verification rules.

## Verification

Use real Git repositories for HEAD and unborn HEAD, tracked and untracked ordinary files, excluded paths, and standalone handle_git. POSIX coverage includes self-links, external regular-file links, an unborn indexed path replaced by a link, and a Git-listed symlink to a FIFO. The FIFO plan regression has an external process deadline and cleanup guard; its child also verifies unscoped handle_git against the FIFO. Existing WorkerRoot unit tests cover linked parent rejection, and the existing target and Lean changed-target oracle suites protect diff semantics. Preserve Windows case behavior and use portable regular-file tests on every platform; gate platform fixtures appropriately.

## Design self-review 1 — requirements and call graph

Reviewed TargetHandler::resolve, handle_git, resolve_changed, and both current-worktree collection callers. A link-only metadata guard would leave excluded regular files being opened and leave the public Git entry unsafe in other ways. Decision: scoped caller plus safe unscoped reader; no public request field additions.

## Design self-review 2 — file identity and async execution

Reviewed WorkerRoot::read, open_parent, is_missing, and Windows reader. An ambient metadata/read sequence has a replacement race; a copied Unix open helper would diverge from Windows behavior. Decision: reuse the capability reader and offload its synchronous work. Missing detection must be typed/structural, not error-message text; InvalidPath skips only after existing Git-path validation.

## Design self-review 3 — compatibility and resource bounds

Reviewed core logical_paths_equal and the empty-explicit behavior in TargetHandler. Raw string equality would regress Windows case matching; treating an empty scope as absent would read excluded links again. Decision: distinguish None from Some(empty), preserve platform equality and Git validation, process one file at a time. Do not shortcut Git repository/revision validation just because no targets remain.
