# Compatibility and change notices

hoimin is currently a 0.x CLI. This policy describes how maintainers classify
changes and tell users what to update. A patch release can fix incorrect
behavior; it does not promise identical mutation candidates or scores.

## Single saved fingerprint input list in 0.10.0

Plan schema 8 stores fingerprint input records only in
`normalized_config.fingerprint_inputs`. The former top-level `fingerprint_inputs`
field is rejected. `sources` continues to record mutation targets separately.
Verify compares current files with the nested list, and review protects those
same files from review-store aliases. Missing lists, duplicate paths, invalid
hashes and invalid paths remain rejected.

Regenerate schema-7 and older plans with this version, the intended project,
environment and original plan arguments, then use the newly generated candidate
IDs for verify or review. Both commands reject old schemas with regeneration
guidance before decoding their old fields. For example:

```console
hoimin plan --root . --source src --fingerprint-file pyproject.toml -- python -m pytest -q > plan-v8.json
hoimin verify plan-v8.json --candidate '<ID_FROM_PLAN_V8>' --format json
hoimin review list plan-v8.json --store reviews.sqlite3 --format json
```

Retain platform-specific options and resource limits from the original invocation
(including `--allow-best-effort-memory` when required on macOS). Keep old files if
needed; hoimin does not convert, overwrite or delete them automatically. Do not
hand-edit schema numbers. Report, session, ranking and fingerprint formats and
fingerprint bytes remain unchanged. The minimum release version is 0.10.0 because
v0.9.4 was already published when this incompatible plan change was prepared.

## Duplicate report configuration fields

Review rejects repeated known fields in a current-schema report's
`run.normalized_config`, including nested fields, regardless of their order or
whether their values agree. Custom report producers must emit each known
configuration field once. Required arrays, null rejection and unknown-field
rules continue to apply to typed run configuration.

This corrects acceptance of invalid input. Current writers emit each field once,
so their output needs no regeneration. Report schema 6, the 0.10.0 release floor,
and other saved formats remain unchanged. Progress still accepts configuration
as an opaque object or null in both JSON and JSONL, without applying review's
configuration or project-root checks. No old-schema adapter or automatic file
conversion is added. See the [implementation and verification record](maintenance/2026-10-16-issue-939.md).

## Required saved configuration arrays in 0.9.0

Normalized saved configurations require explicit `import_roots` and
`fingerprint_files` arrays. Run configuration no longer fills in missing
`import_roots`, and plan configuration no longer fills in missing
`fingerprint_files`. Missing and null arrays are rejected; empty arrays (`[]`)
and nonempty arrays remain supported. Typed run-event readers and review's
report reader enforce the run configuration contract. Verify rejects affected
plans before running a baseline. Progress continues to treat `normalized_config`
as an opaque object or null.

Regenerate plans missing either array. Read reports missing `import_roots` with
their producing binary, or rerun to obtain current output. The array requirement
alone needs no regeneration for data whose writers already include these arrays.
That change retained plan schema 7; the schema-8 change above independently
requires regeneration of old plans. Files are not converted or deleted
automatically. Raw CLI defaults, optional fields, report schema 6, session schema
5, fingerprint schema 11 and fingerprint bytes were unchanged. The minimum
release version for that change was 0.9.0 because v0.8.12 was already published
when it was prepared.

## Typed mutation outcomes in 0.8.0

Report schema 6 replaces independently stored mutation status and optional evidence
with `outcome`: executed results require termination, output state, output, and
elapsed time; reused results permit only killed or survived; `not_run` means no
classified completed result is available and does not imply the process never
started. Status derives from the outcome. Diagnostics do not reconstruct state.
See [Mutation result outcomes](usage.md#mutation-result-outcomes).

Update report consumers to schema 6. Read schema-5 and older reports with their
producing binary, or rerun with the intended version. Session schema 5 stores the
typed outcome directly and rejects initialized schemas 1-4 and unsupported future
versions. Preserve old databases and choose a different `--session PATH` for a new
run. There are no compatibility adapters or automatic file conversions. Mutation outcomes do not change the current plan
format or fingerprint schema 11. The 0.8.0 floor covers these incompatible saved formats because the 0.7.x line
has already shipped.

## Review references and stores in 0.8.0

Review JSON and SQLite review stores use version 2. Plan references contain only
path/hash metadata; report references contain that metadata under `artifact` and
a required `run_id`. A review without a report retains `report: null`. Version-1
stores and old reference shapes are rejected without conversion or fallback.
The 0.8.0 floor reflects this incompatible public review format after the released
0.7 series.

Update strict review JSON consumers to schema 2 and its nested report reference.
Keep old review stores with their producing binary when their history is needed;
record new reviews at a different `--store PATH`. Existing stores are not
converted, overwritten, or deleted automatically. CLI arguments/defaults and
independent report, plan and session formats do not change for this review
refactor; no OS support or package layout change is introduced.
See [review reference and store usage](usage.md#human-review-of-mutation-candidates) for the command
contract and the [review schema](json-schema/review-result.schema.json) for fields.

## Environment snapshots in 0.7.0

Plan schema 7 and report schema 6 retain the `environment` snapshot introduced
in plan schema 6 and report schema 5, replacing the independent environment-name
and optional-hash fields. Untracked inputs use
`{"state":"untracked"}`; captured inputs include `state`, canonical nonempty
`names`, and a lowercase 64-hex `digest`. Captured values remain absent from output.
The 0.7.0 floor reflects this incompatible saved-format and report-reader change.

Regenerate schema-6 and older plans with the intended environment. Update strict
report consumers to schema 6; read schema-5 and older reports with their producing
binary or rerun with this version. Existing files are not automatically converted,
deleted, or replaced. Fingerprint schema 11 and its bytes are unchanged, so the
environment representation change alone does not invalidate compatible sessions;
the typed-outcome session schema change described above independently requires a
new database. If a run is
rejected for another compatibility reason, choose a different `--session PATH`.
See [Explicit inherited environment fingerprints](development.md#explicit-inherited-environment-fingerprints).

## Saved changed selection in 0.8.0

The release floor is 0.8.0 because this saved-format and public report-reader
redesign is incompatible with the published 0.7 series. The latest stable
release checked for this change was v0.7.3.

Plan schema 7 and report schema 6 encode normalized `changed` as null when disabled, or as an object
containing `context` and optional `diff_base` when enabled. The former boolean
`changed` and sibling `changed_context` / `diff_base` fields are rejected.
Regenerate plans containing the former representation. Read historical reports
with their producing binary, or rerun to obtain current output; regenerating a
plan does not recover a past report. Files are not automatically converted or
deleted. CLI flags, Git comparison semantics, and effective target fingerprints
retain their meaning.

## Typed session intent in 0.8.0

Report schema 6 replaces normalized `session`/`resume` pairs with one `session`
object. Disabled sessions use `{"mode":"disabled"}`. Fresh and resumed sessions
use `{"mode":"fresh","config":{"path":"..."}}` and `{"mode":"resume","config":{"path":"..."}}`.
The raw CLI options remain `--session` and `--resume`. Normalized inputs reject
old session pairs and the obsolete `resume` field; no conversion is attempted.
Read older reports with their producing binary or rerun with this version.
The minimum release version is 0.8.0: the 0.7 series is already published, so
this incompatible report/configuration change starts a new minor series.
Regenerate saved plans with the intended binary and inputs; update report
readers to schema 6. Runtime session intent remains absent from plan schema 7; typed outcomes
require session database schema 5. Existing session compatibility checks still apply;
choose a new session path if those checks reject an old session.

## Validated verification selection in 0.8.0

Verification metadata retains its current JSON shape and schema numbers. Readers
now reject known fields that disagree: mode/policy/scope mismatches, sampling on
non-sampled selections (including null), missing sample evidence, truncated sample
populations, zero requests, selected counts above requests, and empty, duplicate
or oversized sample IDs. A sample's selected count must equal its ID count.
Reports continue to accept non-sample selected=0 and additional outer metadata.
The nested sampling object retains its documented closed shape.

Correct writer-produced data needs no conversion. For contradictory hand-built
metadata, regenerate the report or correct the producer; readers do not guess or
repair provenance. Current preview/report schemas express the mode relationships;
Rust additionally checks cross-field counts. The existing 0.8.0 release floor
is unchanged. Internal Rust callers use validated `VerificationSelection`
constructors and read-only accessors instead of public struct fields.

## Required session budgets in 0.8.0

New session databases require a non-NULL positive eight-byte BLOB mutant budget.
An existing schema-5 database with valid budgets still opens and resumes without
DDL conversion. A NULL budget in a current database created with earlier,
nullable DDL now produces `session.corrupt` instead of the former budget-specific
`session.resume.incompatible`. Invalid rows are rejected without repair; preserve
the database for inspection and choose a different `--session PATH` for a fresh run.

Normal stored bytes, session schema 5, CLI behavior and the 0.8.0 release floor
remain unchanged. No migration is needed for valid current databases. Unsupported
schemas 1–4 remain rejected; this change does not restore legacy migration paths.

## Public interfaces

| Interface | Maintenance rule | User action when it changes |
| --- | --- | --- |
| CLI arguments, defaults and exit codes | Preserve documented usage within a minor series, except documented bug fixes. Removing or changing a supported contract needs a minor-version floor increase. | Update invocation and CI conditions using the release notice. |
| JSON and JSONL reports | Treat documented schemas as public. Progress accepts only report schema 6, matching the writer; review accepts current-schema JSON. Adding a field may still affect strict readers. | Read old reports with their producing binary, or rerun with the intended version. Plan regeneration alone cannot reconstruct past reports. Files are not automatically converted. |
| Saved plan, ranking and candidate IDs | These are checked execution inputs, not timeless interchange formats. Changes to schema, ranking, identity, source or fingerprints can invalidate a plan. | Regenerate the plan with the intended version and inputs. Do not hand-edit versions or IDs to bypass validation. |
| Session database and resume | Resume is allowed only under the implementation's schema and fingerprint checks. Migration support must be stated explicitly. | Keep the old database and binary if needed; start a new session when compatibility is not supported. |
| Default operators, profiles and selection | Changes can alter candidate count, runtime and the population behind a score. Default expansion is an observable compatibility change. | Review the operator set and budget; use matching candidates and validation settings for comparisons. |
| Supported Python, OS and architecture | A support removal or higher runtime minimum requires a minor-version floor increase and migration notice. | Keep a compatible release or update the environment. |
| Archive and wheel layout, installation and runtime dependencies | Changes to the distributed package contract require a minor-version floor increase. | Follow the installation or packaging migration steps. |
| Internal Rust APIs, caches and diagnostic logs | No independent stability guarantee. Versioned reports are distinct from diagnostic output. | Use the documented CLI and report schemas; do not parse log wording as a stable protocol. |

See the [usage guide](usage.md) for runtime contracts and the generated
[CLI reference](cli-reference.md) for argument definitions. This policy does
not make a newer binary accept an older schema or weaken input validation.

## Session load outcomes in 0.7.0

The 0.7.0 floor removes the fallback report reason `no_compatible_run`.
Report schema 6 retains the six reasons produced by session selection; the removed
reason described only externally supplied legacy completion events. Read reports
containing that reason with their producing binary, or rerun with this version.

Rust event callers construct `SessionLoaded { id, outcome }` with
`SessionLoadOutcome::Resumed(SessionResumeRef)` or `Fresh(ResumeFreshReason)`.
The old `resume`/`fresh_reason` fields and missing outcomes are rejected. Update
event producers; no compatibility adapter supplies a reason. This changes no
SQLite, fingerprint or plan format, so this change alone requires no new session
or plan regeneration. Existing schema/fingerprint compatibility checks still apply.
Existing files are retained without automatic conversion.

## Report reader support in 0.6.0

The 0.6.0 version floor removes progress support for schema-2 JSON and schema-3
JSON/JSONL. Schema-4 JSON and JSONL remain supported. Keep the old binary with
historical reports or rerun with the intended version; do not hand-edit schema
numbers. This is a reader-support removal; report output remains schema 4.

## Canonical saved enum names in 0.7.0

Saved typed configurations accept only `auto` / `fixed`, `unix` / `windows`,
and `type_dict_mapping`. The former aliases `Auto`, `Fixed`, `Unix`, `Windows`
and `type_mapping` are rejected. Regenerate plans containing old names; do not
hand-edit saved inputs to bypass validation. Read historical reports with their
producing binary or rerun to obtain current output. Plan regeneration alone
does not recover past reports. Canonical output shapes are unchanged.

## Choosing the release floor

The manifests contain a minimum version, not the next exact patch number.
[Release automation](releases.md#github-releases) chooses the higher of that
floor and the next patch after the highest stable tag, with the existing history
and atomic reservation checks. Changing the floor must keep `Cargo.toml`,
`pyproject.toml`, `Cargo.lock`, `uv.lock` and `fuzz/Cargo.lock` consistent.

| Change | Version decision and notice |
| --- | --- |
| Documentation, CI-only checks, SBOM or provenance metadata without a package-contract change | Keep the current minor series. Explain any operational change. |
| Bug fix restoring a documented contract, including removal of invalid mutation candidates | Patch in the current minor series. Describe changed results and any required plan regeneration. |
| New operator available only by explicit selection, with existing defaults and saved-format contracts intact | Patch in the current minor series; name the opt-in and its scope. |
| Expanding or changing the default operator set/profile | Raise the floor to a new minor series. Explain candidate, budget and score-comparison effects. |
| Removing a CLI option, changing a supported exit-code contract or making a public schema incompatible | Raise the minor-version floor and give concrete invocation/reader migration steps. |
| Ranking/identity algorithm or saved-format change that invalidates supported saved inputs | Raise the minor-version floor; specify whether to regenerate plans or start new sessions. A bug fix rejecting previously invalid inputs is distinguished from a format redesign. |
| Changing the archive/wheel package contract or required runtime environment | Raise the floor to a new minor series and document installation changes. |

For example, with a highest tag of `v0.3.9`, a floor of `0.3.0` yields
`v0.3.10`; a floor of `0.4.0` yields `v0.4.0`. If `v0.4.0` already exists,
that floor yields the next patch in that series. Merely updating the floor
does not migrate saved data. Never rewrite published tags or replace published
files to repair a release.

## Owned Rust writer APIs

The internal Rust entry `run_with_io` is replaced by `run_with_writers`, which
accepts owned `Write + Send + 'static` outputs. Shell run entry points and
`VerifiedPlan::execute` require the same ownership. Shared collecting writers
can supply snapshots after execution. Shell errors have already been reported;
callers must not print them again. See the [developer migration](development.md).

This internal Rust source migration does not require a version-floor increase.
CLI arguments, exit codes, report schemas, plans and sessions do not change;
existing files require no conversion or deletion.

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

## Session initialization change (Issue 882)

Issue 882 originally raised the release floor to 0.6.0 by removing automatic
upgrades from schemas 1-3. At that revision, new databases received schema 4 and
existing schema-4 databases reopened. The typed-outcome schema-5 contract above
supersedes that schema-4 policy: initialized versions 1-4 are now rejected.
Keep old databases with their producing binary, and choose a different
`--session PATH` for a new run. hoimin does not transform, remove or replace the
old database. See [Sessions and resume](usage.md#sessions-and-resume).
