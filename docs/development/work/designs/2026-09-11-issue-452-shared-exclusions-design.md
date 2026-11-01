# Issue #452: Shared workspace exclusions

## Contract

Automatic target discovery and worker copying must share the built-in excluded entry names. The root itself is always traversable, including when named `venv`. Includes restore ignored or hidden files but never built-in exclusions. Explicit file and line selectors inside an excluded path fail during target resolution with the path and a recommendation to choose source outside the excluded directory. Source-root scans and include globs may match no eligible files. User excludes still win over includes.

## Cause and implementation

The workspace manifest prunes `.git`, virtual environments and caches in both walks; target discovery does not. Move that exact predicate to `copy_policy.rs`, expose crate-private entry and relative-path checks, and use the entry predicate in both target walks and both copy walks. Validate normalized explicit file/line selectors in TargetHandler before discovery using the same names. Set target discovery's `require_git(false)` to match workspace interpretation of .gitignore in non-Git roots. The shared name comparison ignores ASCII case on Windows and is exact elsewhere. This also excludes uppercase Windows virtual-environment/cache names that the previous copier accidentally allowed. Keep target hidden defaults and explicit restoration, symlink behavior and user override precedence.

Duplicating the list would allow another divergence. Copying virtual environments would violate the established isolation policy. Skipping the manifest validation would conceal a selected mutation that cannot be executed. A shared policy at discovery boundaries addresses the mismatch.

## Validation and limits

Use non-Git fixtures with root and nested excluded directories, includes, hidden files and ignore files. Confirm direct selectors fail with actionable diagnostics. Exercise plan, run and verify against a root containing a valid source plus venv/dep.py; only the eligible source enters candidates and the run reaches baseline. Existing workspace exclusion tests preserve copy behavior. Lean is unnecessary for the directory-entry predicate; real walker and CLI correspondence tests are the relevant evidence.

## Design self-review

1. Requirements: automatic scans, explicit file/line failures and non-overridable include behavior cover the Issue; no change permits cache copying.
2. Boundaries: use normalized root-relative selectors, exclude by path components, preserve the root exception; compare exclusion names case-insensitively on Windows and exactly elsewhere, matching platform target resolution.
3. Integration: share the policy without coupling target to workspace internals; align ignore handling in non-Git roots, retain hidden selection and exclude precedence.
