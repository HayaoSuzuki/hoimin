# Issue #753 compatibility policy

Base: `79b6bde29b05e9904c8efff59e02d0c45377d7f7`.
Branch: `docs/issue-753-compatibility-policy`.

## Design and implementation plan

Keep this as a documentation/template change. Add one canonical compatibility
policy, link it from usage and release guidance, and expose it through the OKF
catalog. Keep the tag allocator and TestPyPI/PyPI pipeline unchanged. Review the
policy against current code and validate local links, OKF structure and the
existing release-floor tests. Do not add prose assertions or documentation E2E.

1. Write the public-interface table and distinguish bug fixes from contract changes.
2. Describe the existing floor allocator, five-file update and immutable releases.
3. Add a PR migration field and an illustrative release notice.
4. Add the sourced OKF concept and index link; validate format, links and hashes.
5. Run relevant existing allocator tests and diff checks, then commit and open a PR.

## Design self-reviews

1. CLI, reports, saved plans/ranking/IDs, sessions, defaults and runtime support each
   have an explicit rule; internal logs are not promoted to a public schema.
2. A bug fix can change candidates within 0.3.x; score stability is not promised.
   Default expansion and supported-contract removals require a minor floor.
3. The floor is not an exact release number: examples preserve next-patch selection.
4. Saved data is not migrated by policy text. Current validation remains authoritative.
5. Package metadata/CI checks and package-contract changes are distinct; published
   files stay immutable and TestPyPI remains the production gate.

## Plan self-reviews

1. One policy is the source of truth; usage and releases link instead of duplicating it.
2. The PR template covers interface, version and migration without a new validator.
3. The generated CLI reference is left generated; no hand-edited policy text is added there.
4. Existing allocator tests cover the factual floor behavior; new wording tests add no value.
5. The new OKF concept needs source hashes and an index link; no implementation or
   audit-report catalog entry is required for this review document.

## Implementation self-reviews

1. The policy table includes all requested contracts and avoids guaranteeing legacy schemas.
2. All five version files, including the separate fuzz lockfile, are named explicitly.
3. The notice uses a clearly fictional operator and does not claim an unreleased feature shipped.
4. The stale release-guide sentence claiming no automatic PyPI publication is corrected
   to agree with the existing pipeline; no workflow behavior changes.
5. Diff scope is documentation and template only; no runtime dependencies, manifests,
   release bytes or secrets are changed. Version floor stays 0.3.0/current patch series.

## Verification self-reviews

1. Check all changed local links resolve, including new policy and OKF index links.
2. Check OKF YAML/reserved files and new source IDs, footnotes and raw-byte hashes.
3. Run existing floor tests and inspect their results instead of claiming prose proves code.
4. Check the patch introduces no executable workflow or package change.
5. Check whitespace and branch identity before commit; preserve untracked `.idea/`.

## Results

Existing release-floor/tag-reservation tests: 5 passed, 44 deselected. The first
sandboxed attempt could not create pytest temporary directories; the authorized
rerun passed. OKF validation covered 36 pages; changed local links and current-base
source hashes passed. `git diff --check` passed. Free disk space was 189.6 GB.
An independent reviewer found no substantive issues; its housekeeping finding
was resolved by recording these results. No new tests are added for this
documentation change.
