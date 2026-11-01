# Issue 619: explicit source existence

Explicit `--source` input must identify an existing path before target discovery or Git intersection. A typo must produce exit 2 and identify the original input before the test command runs. Empty directories and existing sources filtered to zero candidates remain successful. Existing Python file sources remain supported.

Implement the check in `TargetHandler::resolve`, shared by run, plan, and saved-plan verification. For every source, call the existing lexical `normalize_logical_path`, then query metadata at root joined with the normalized relative path. Do not infer existence from discovered candidates. NotFound gets a specific missing-source diagnostic; other filesystem errors remain actionable errors containing the source and OS message. Quote user-supplied paths with debug formatting to keep embedded newlines unambiguous. Preserve normalizable parent components and root-equal paths; outside-root selectors retain their existing error.

Metadata follows a link only to determine whether its target exists. This does not change discovery's no-follow policy or permit reading linked content. Dangling links are missing. The check is an early diagnostic, not a race-free filesystem guarantee: later discovery still owns read errors. No configuration, plan schema, or pure core semantics changes are needed. Lean would model a supplied existence boolean without verifying filesystem observation, so use real filesystem and public CLI regression cases instead.

## Design self-reviews

1. Call-path review: run/plan/verify use TargetHandler; putting validation before discovery covers mixed valid/missing sources and changed-clean repositories. Kept the check outside candidate filtering.
2. Compatibility review: preserve regular files, empty directories, `.` and root-absolute paths. Normalize before filesystem access so `absent/../src` follows established lexical semantics. Deliberately retained no-follow discovery and made dangling-link diagnosis explicit.
3. Failure review: do not use exists(), which hides permission errors. Preserve OS errors, include original input, and escape control characters. No claim of preventing a source disappearing after validation.
