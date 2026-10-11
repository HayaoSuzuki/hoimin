# Compatibility and change notices

hoimin is currently a 0.x CLI. This policy describes how maintainers classify
changes and tell users what to update. A patch release can fix incorrect
behavior; it does not promise identical mutation candidates or scores.

## Public interfaces

| Interface | Maintenance rule | User action when it changes |
| --- | --- | --- |
| CLI arguments, defaults and exit codes | Preserve documented usage within a minor series, except documented bug fixes. Removing or changing a supported contract needs a minor-version floor increase. | Update invocation and CI conditions using the release notice. |
| JSON and JSONL reports | Treat documented schemas as public. Readers must check the schema version; adding a field may still affect strict readers. | Update readers before consuming the new format; retain old reports with their producing version. |
| Saved plan, ranking and candidate IDs | These are checked execution inputs, not timeless interchange formats. Changes to schema, ranking, identity, source or fingerprints can invalidate a plan. | Regenerate the plan with the intended version and inputs. Do not hand-edit versions or IDs to bypass validation. |
| Session database and resume | Resume is allowed only under the implementation's schema and fingerprint checks. Migration support must be stated explicitly. | Keep the old database and binary if needed; start a new session when compatibility is not supported. |
| Default operators, profiles and selection | Changes can alter candidate count, runtime and the population behind a score. Default expansion is an observable compatibility change. | Review the operator set and budget; use matching candidates and validation settings for comparisons. |
| Supported Python, OS and architecture | A support removal or higher runtime minimum requires a minor-version floor increase and migration notice. | Keep a compatible release or update the environment. |
| Archive and wheel layout, installation and runtime dependencies | Changes to the distributed package contract require a minor-version floor increase. | Follow the installation or packaging migration steps. |
| Internal Rust APIs, caches and diagnostic logs | No independent stability guarantee. Versioned reports are distinct from diagnostic output. | Use the documented CLI and report schemas; do not parse log wording as a stable protocol. |

See the [usage guide](usage.md) for runtime contracts and the generated
[CLI reference](cli-reference.md) for argument definitions. This policy does
not make a newer binary accept an older schema or weaken input validation.

## Choosing the release floor

The manifests contain a minimum version, not the next exact patch number.
[Release automation](releases.md#github-releases) chooses the higher of that
floor and the next patch after the highest stable tag, with the existing history
and atomic reservation checks. Changing the floor must keep `Cargo.toml`,
`pyproject.toml`, `Cargo.lock`, `uv.lock` and `fuzz/Cargo.lock` consistent.

| Change | Version decision and notice |
| --- | --- |
| Documentation, CI-only checks, SBOM or provenance metadata without a package-contract change | Keep the current minor series (currently 0.3.x). Explain any operational change. |
| Bug fix restoring a documented contract, including removal of invalid mutation candidates | Patch in the current minor series. Describe changed results and any required plan regeneration. |
| New operator available only by explicit selection, with existing defaults and saved-format contracts intact | Patch in the current minor series; name the opt-in and its scope. |
| Expanding or changing the default operator set/profile | Raise the minor-version floor (next: 0.4.0). Explain candidate, budget and score-comparison effects. |
| Removing a CLI option, changing a supported exit-code contract or making a public schema incompatible | Raise the minor-version floor and give concrete invocation/reader migration steps. |
| Ranking/identity algorithm or saved-format change that invalidates supported saved inputs | Raise the minor-version floor; specify whether to regenerate plans or start new sessions. A bug fix rejecting previously invalid inputs is distinguished from a format redesign. |
| Changing the archive/wheel package contract or required runtime environment | Raise the minor-version floor (next: 0.4.0) and document installation changes. |

For example, with a highest tag of `v0.3.9`, a floor of `0.3.0` yields
`v0.3.10`; a floor of `0.4.0` yields `v0.4.0`. If `v0.4.0` already exists,
that floor yields the next patch in that series. Merely updating the floor
does not migrate saved data. Never rewrite published tags or replace published
files to repair a release.

## PR and release notices

Fill in the PR template's compatibility section before merging. Describe:

- which public interfaces change, or why there is no user-visible impact;
- the selected version floor and why it fits the table above;
- old and new commands/formats, and any plan/session/reader migration;
- changed defaults, support requirements and score-comparison limits.

The existing workflow generates GitHub release notes from merged PRs. Use a
clear PR title and description; when editing the generated release description,
organize the relevant entries under **Added**, **Fixed**, and
**Compatibility and migration**. The generated notes are not an automated
compatibility assessment, and this change adds no release-note generator.
PyPI publication continues through the existing TestPyPI gate.

An illustrative notice for a future change is:

```markdown
## Added
- Added an explicitly selected operator `example_operator`.

## Fixed
- Stopped proposing invalid candidates in a documented unsupported context.

## Compatibility and migration
- The default profile now includes `example_operator`; the version floor is 0.4.0.
- Recreate saved plans to use the new default population. Compare scores only
  after matching candidate selection and validation settings.
- Existing sessions are not automatically upgraded by this release. Preserve
  them with their producing binary and start a new session if rejected.
```

The operator and changes above are examples, not claims about a shipped release.

## Before merging a release-affecting PR

1. Classify the interfaces and default behavior affected by the final diff.
2. Check the version floor and all five manifests/lockfiles if it changes.
3. Include the migration notice and link the updated usage/schema documentation.
4. Run checks for the changed contract; reuse existing tests rather than adding
   wording checks or a separate documentation E2E suite.
5. Preserve the existing validated-artifact, provenance and TestPyPI/PyPI gates.
   A failed check must not be bypassed to ship a compatibility change.

This is a review checklist, not a claim of formal compatibility verification.
No retrospective rewrite of reports, session databases or old release notes is
required. Revisit the policy before a 1.0 release or a new distribution interface.
