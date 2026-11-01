# Issue 606: protect verify manifests from metrics replacement

The verify command must reject metrics destinations that replace its input manifest before baseline execution, preserving manifest bytes and preventing test-command markers. Both `--top` and `--candidate`, and both owned CLI and borrowed-writer dispatch, must enforce this. Distinct metrics files, existing sidecars, failed-baseline metrics, and separate final symlink/hardlink entries remain supported.

Reuse the existing filesystem directory-entry inspector. Preserve the absolute specified manifest path and its canonical referent in `VerifiedPlan`, without putting invocation-local paths in serialized run configuration. Pass those protected paths through a shared verified shell entry point into `ShellContext` and its preflight task. Extend metrics destination inspection with these extra protected paths. This retains existing collision diagnostics, warning behavior for uncertain identity, and the resolved atomic rename destination.

Canonicalizing only metrics would incorrectly reject safe symlink aliases. Comparing file inodes would incorrectly reject hardlinks. A new independent CLI path-comparison check would duplicate filesystem alias logic and could diverge between dispatch modes. The selected approach uses the established preflight boundary and does not change the plan schema.

Protect both the supplied final symlink entry and the resolved manifest file. Other aliases remain replaceable. Parent symlinks, absolute/relative paths, dot components, and native filesystem case aliases use existing entry identity rules. This change does not promise protection against concurrent hostile filesystem replacement during execution.

## Design self-reviews

1. Data flow: manifest path is currently discarded; storing it only in CLI dispatch would miss a verified shell consumer. Decision: retain two paths on VerifiedPlan and use common shell setup.
2. Filesystem semantics: canonicalizing the destination or inode equality would forbid safe aliases. Decision: protect specified entry plus canonical input, retaining no-follow destination inspection.
3. Error/lifecycle review: early ad hoc validation could change warning behavior or miss preflight ordering. Decision: add inputs to existing preflight validation; no baseline/session side effects precede a confirmed collision. No further design gaps found.
