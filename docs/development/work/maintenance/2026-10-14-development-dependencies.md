# Development dependency update follow-up (#862)

Base: `0a545ea` (main). Branch: `fix/development-dependencies-862`.
Imported Renovate revision: `f7ad86df90843acb3ebe41038be85c4fecb7b735`.
Remote: `https://github.com/HayaoSuzuki/hoimin.git`; PR target: `main`.
The replacement PR includes the dependency update and fixes; Renovate's branch
is not modified. This record separates local checks from hosted evidence.

## Diagnosis and scope

CI run `38470168447` failed on a Python-version assertion and eight ty 0.0.86
`truthiness-test-of-none-union` diagnostics. Both reproduced locally before the
fix. Python 3.15 workflow inputs contradicted `requires-python >=3.14,<3.15` and
the existing CI contract. Keep 3.14 and constrain Renovate's
`actions/python-versions` package to that minor series. This is not a Python
support expansion. The official Renovate github-actions extractor identifies
this package separately from the setup-python Action itself.

Use explicit `bool` for intentional falsy handling: missing and empty paths,
empty package names and empty shape lists keep their old behavior. `which`
returns a path or None, so use an explicit absence check. No global diagnostic
suppression or optional-value helper is introduced.

Renovate's artifact warning states that resolving the broad Hypothesis range
selected pending 6.169.0 instead of intended 6.168.5. Pin both dev/fuzz groups
to 6.168.5 and regenerate the lock. Other Python updates, proptest 1.12.0 and
Action/uv updates from the source PR are retained.

## Design self-reviews

1. Matched each failed hosted step with its log and local reproduction; separated
   the optional-value diagnostics from the workflow contract assertion.
2. Compared Python inputs with the package range, lock range, Ruff/ty targets
   and existing wheel contract before choosing the supported interpreter.
3. Compared truthiness with None-only tests: empty environment values and lists
   must retain fallback behavior; use bool where truthiness is intentional.
4. Checked the Hypothesis artifact warning against both dependency-group entries
   and the resolved lockfile; exact matching pins constrain the resolver.
5. Reviewed ownership: main remains the PR base, all edits belong to a dedicated
   branch, and the Renovate ref is read-only throughout this task.

## Planning self-reviews

1. Selected local ty and the existing Python-version assertion as the red checks;
   no synthetic test of diagnostic wording is needed.
2. Planned full Python tests for the updated toolchain and affected helpers,
   plus workflow linters and the official Renovate configuration schema.
3. Planned all-feature Rust tests, including existing proptest and Lean-oracle
   adapters, because the proptest update changes its transitive graph.
4. Included both-workspace locked metadata and build-script inventory review,
   per dependency policy; preserve the unchanged fuzz lockfile.
5. Set two build/test workers, a dedicated target and sampled memory/disk/time
   guard; serialize expensive Cargo operations and clean at completion.

## Implementation self-reviews

1. Reviewed all workflow diffs: updated Action digests and uv version remain;
   Python inputs return to 3.14 without altering triggers or permissions.
2. Checked the Renovate rule matches `actions/python-versions`, not the
   setup-python Action digest; its npm version range admits 3.14 only.
3. Compared each optional-value expression before/after, including walrus
   assignments and package-relative imports; runtime branching is preserved.
4. Regenerated uv.lock with uv 0.13.0: only Hypothesis changed relative to the
   imported lock, from 6.169.0 to 6.168.5, plus its manifest specifiers.
5. Regenerated build inventory: main graph has 218 packages and 53 build scripts;
   fuzz stays at 93 packages and 19 scripts. Main adds borsh 1.8.1 and removes
   getrandom 0.3.4 and wit-bindgen 0.57.1 scripts. Read borsh's build.rs: it
   declares a hash-collection cfg alias; this is not a general safety audit.

## Verification self-reviews

1. Read both red outputs: eight ty diagnostics and the expected 3.15/3.14
   assertion failure; after fixes both commands pass.
2. Read the full Python result: 993 passed and eight skipped on CPython 3.14.7;
   the skipped fixtures are not counted as native execution evidence.
3. Checked workflow lint ran with the installed tool executables on PATH;
   actionlint, shellcheck and zizmor pass with existing ignored findings.
   Zizmor's existing Python-shell analysis limitations are not new coverage.
4. Checked official Renovate JSON-schema validation and manifest/lock agreement;
   schema validation is not a hosted Renovate bot execution.
5. Keep remaining Rust, review and PR checks separately recorded below;
   local results cannot certify hosted artifact transport or other OSes.

## Design assessment and formal methods

The bounded optional-value interface assessment is 6/8 diagnostics (7.5/10)
initially: intent and newcomer-readable empty-value behavior are obscured by
implicit tests. Explicit bool/absence tests address those two rows. Final
assessment is 8/8 (10/10) for these expressions: one responsibility per helper,
no new interface or leaked state, local decisions, visible intent, complexity
review and maintenance attention. This does not score the whole release system.

There are no state-machine, ownership or deadline changes requiring a new Lean
model. Existing property tests and Lean-oracle adapters run with the updated
dependencies. A new mutation campaign would not distinguish the intentional
boolean normalization from the old expression; it is not required for this
syntax-preserving adaptation. This is not a mutation-score claim.

## Compatibility and evidence

The version floor stays 0.5.0. CLI, report/session formats, defaults, package
layout and supported Python/OS ranges are unchanged; no user migration is
required. Developers receive updated tools and exact Hypothesis pins. Native
Windows/Linux tests and hosted artifact transport remain CI responsibilities.

Consulted the development guide, dependency policy and build inventory, OKF
workflow, dependency-policy concept and release-provenance concept. Existing
release evidence is historical; Action updates do not repeat an attestation
experiment. Validation logs are local `/tmp/hoimin-862-*.log` and `.summary`
artifacts, not repository fixtures.

## Additional verification

Ruff format/check and ty pass with ruff 0.17.0 and ty 0.0.86. `uv lock --check`
passes under uv 0.13.0. All 38 literal setup-python inputs satisfy the package's
Python range, and both Hypothesis manifest pins match the lock. Comparing the
five changed helper ASTs after removing bool wrappers found no other changes.

Workflow lint passes with existing ignored findings. The official Renovate JSON
Schema accepts the new rule. Python audits for Linux, macOS and Windows each
report no known vulnerabilities or adverse project statuses in 84 packages.
The strict docs build passes, as do OKF YAML/reserved-file checks and all three
new source hashes/footnotes. Local checks do not execute a hosted Renovate update.

The Rust lockfile is byte-identical to the source PR revision whose dependency
policy and vulnerability audit jobs passed in hosted run `38470168480`. This
is reused hosted evidence, not a new local cargo-deny/cargo-audit execution.
The fuzz lockfile remains unchanged. No license/source exceptions were loosened.

Independent read-only review found no actionable issues. It checked workflow
inputs, dependency pins, code semantics, Action diff scope, inventory, source
hashes and the Python execution log. It did not run builds or extend the result
to native platform execution, hosted artifact transport or a full security audit.

Official Renovate references consulted: [GitHub Actions manager](https://docs.renovatebot.com/modules/manager/github-actions/),
[Python input mapping](https://github.com/renovatebot/renovate/blob/main/lib/modules/manager/github-actions/known-actions/github-releases.ts),
and [range strategies](https://docs.renovatebot.com/configuration-options/#rangestrategy).
Exact pins remain updateable; the existing update-lockfile strategy replaces
out-of-range requirements during future updates.

## Final Rust verification

`cargo test --locked --workspace --all-features` passed 2,831 tests with
29 ignored across 152 top-level test targets on macOS. The count
excludes nested subprocess summaries. Existing proptest and Lean-oracle adapters
are included; no new formal theorem is asserted. Guard: exit 0, 372.9 seconds,
peak process-group RSS 1,705,000,960 bytes, no resource stop.

Resource limits were sampled at 8 GiB process-group RSS, 12 GiB target usage,
20 GiB minimum free space and 40 minutes per command. Builds used two workers
and tests two threads, with incremental/debug artifacts disabled. These are
sampled limits, not OS-enforced aggregate quotas.
