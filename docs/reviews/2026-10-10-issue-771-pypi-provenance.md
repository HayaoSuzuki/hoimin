# Issue #771: PyPI build provenance and automatic publication

## Approved design

After a merged main-branch release, `release.yml` publishes the GitHub Release,
publishes its exact wheels to TestPyPI, retrieves and verifies all three wheels
and both attestations, and only then publishes to PyPI and verifies it too.
The index publication phase uses a separate `workflow_dispatch` run on main;
PyPI rejects upload tokens from the original `pull_request_target` build event.
Both index upload jobs are directly in `release.yml`: a reusable upload workflow
would change `job_workflow_ref` and fail the registered publisher lookup.
Each wheel gets a separate build-time SLSA statement. The existing 13-subject
GitHub attestation remains available. Signed statements are never rewritten.
The publisher and build certificate both use `release.yml`; each index needs a
Trusted Publisher registration for that workflow. The upload jobs have only
OIDC permission and use protected environments. Python remains 3.14.

Automatic upload is opt-in through the repository variable
`PYPI_AUTO_PUBLISH=true`, so merging before publisher configuration does not
cause an upload attempt. Manual recovery also runs from `release.yml` on main,
uses original single-subject build attestations, and never rebuilds wheels.
The previous manual workflow is retired to prevent an alternate ungated path.
Only metadata changes: retain the 0.3.x version floor and existing release
asset inventory. Existing PyPI files cannot gain attestations retroactively.

## Design self-reviews

1. **Subject count:** PyPI's verifier requires exactly one subject. Keep the
   aggregate GitHub statement and create three independent wheel statements.
2. **Identity:** Warehouse verifies every statement against the uploading
   publisher. Merely adding another publisher does not make a cross-workflow
   statement valid. Both build and upload must use `release.yml`.
3. **Authenticity:** Convert Sigstore to PEP 740 without modifying signed bytes;
   verify source commit, workflow revision, hosted runner and wheel digest first.
4. **Rollout:** The user may be unable to configure publishers now. Default off;
   provide exact setup values and enable only after both registrations exist.
5. **Failure semantics:** TestPyPI verification gates production. A retry may
   reuse a complete publication only after both signatures and bytes verify;
   partial/mismatching publication fails rather than silently skipping files.

## Implementation-plan self-reviews

1. **Dependency order:** Add failing tests for attestation conversion, identity
   policy, index verification, retry decisions and workflow gating before code.
2. **Trust boundary:** Selection metadata is untrusted; cryptographic verification
   must succeed before staging. Downloaded files are data; upload jobs execute
   no release code.
3. **Persistent recovery:** Retrieve original build bundles from GitHub's
   attestation service by wheel digest, not expiring Actions artifacts.
4. **Coverage:** Exercise missing, malformed, multiple-subject, wrong predicate,
   source/workflow/repository mismatch, corrupt signature, changed bytes,
   incomplete publication and API failures. Use external-process doubles only
   for external services; distinguish local orchestration from hosted proof.
5. **Completion:** Strict Python and workflow lint, full Python tests, bounded
   mutation checks, independent review and committed documentation precede PR.
   Hosted TestPyPI acceptance remains pending until merge and configuration;
   do not claim it passed locally or automatically close the issue early.

## Official sources inspected

- [PyPI attestations](https://docs.pypi.org/attestations/)
- [Production and conversion](https://docs.pypi.org/attestations/producing-attestations/)
- [Integrity API](https://docs.pypi.org/api/integrity/)
- Warehouse `f43a0f79dbaf40e4888110ad5b1d5e0ad87121b6`,
  `warehouse/attestations/services.py`: verification uses the upload publisher.
  `warehouse/oidc/models/github.py`: publisher lookup uses `job_workflow_ref`,
  and `pull_request_target` is an unsupported upload event.
- pypi-attestations `0838c9fe75b77ef1ee9053cc73ef91454c11b453`,
  `src/pypi_attestations/_impl.py`: exactly one subject; Build Config URI policy.
- PyPA publishing action `dc37677b2e1c63e2034f94d8a5b11f265b73ba33`:
  generates `*.publish.attestation`, refuses existing publish statements, and
  uploads adjacent attestations through Twine.

## Validation results

- Full Python suite: `python -m pytest -q --tb=short`:
  **896 passed, 28 skipped**, 263.07 seconds on Windows / Python 3.14.
- Strict Ruff and ty checks passed. All workflows passed actionlint, ShellCheck
  and zizmor; only the three existing, explained exceptions remain.
- `cargo build --locked --bin hoimin` passed for working-tree mutation checks.
- Bounded mutation verification of `statement_types` and `check_inventory`:
  **8 killed, 0 survived, 0 errors**, run
  `232277b6-1028-453d-a419-e335b57baa0a`. Selected equality, boolean and
  membership mutations at lines 56, 64, 82, 83 and 88. Fresh baseline passed;
  jobs 1, workspace limit 8 GiB, free-space reserve 10 GiB. Peak owned workspace
  was 39,061,528 bytes; minimum free space was 188,742,778,880 bytes.
  Workers and the exact external temporary evidence directory were cleaned.
  An earlier attempt stopped when concurrent documentation editing changed the
  original tree; restarted with edits paused rather than ignoring the guard.
- OKF YAML / reserved-file validation passed for all 35 knowledge Markdown
  files; updated concepts' links, footnotes and refreshed source hashes passed.
- Final `cargo clean` removed 2,580 files / 2.2 GiB; remaining free space was
  190,824,292,352 bytes. Earlier clean ran before the working-tree CLI build.
- Independent review identified three issues documented below. After fixes,
  the reviewer found no remaining blocking findings and confirmed the pinned
  PyPA action / Twine handling of both adjacent attestation files.
- Actual OIDC exchange, new single-wheel signature issuance, TestPyPI upload,
  and production upload remain **unverified** until merge and account setup.
  No publisher, environment, or automatic-publication variable was changed.

## Implementation self-reviews

1. **OIDC event:** Independent review found that PyPI rejects
   `pull_request_target`. Confirmed `_check_event_name` in Warehouse, added a
   merge-only opt-in dispatch job, and restricted preparation to dispatch/main.
2. **OIDC workflow:** Independent review found that a reusable upload changes
   `job_workflow_ref`. Confirmed Warehouse publisher lookup, removed the reusable
   workflow and put both upload jobs directly in `release.yml`. The final
   workflow needs no new lint exclusions.
3. **Retry evidence:** Independent review found that exact-one-candidate rejects
   original-build reruns. Reproduced with a failing duplicate test; now verify
   all matching original candidates and select one deterministically.
4. **Byte integrity:** Staging validates all wheels and signatures in a temporary
   directory before copying output. Official conversion preserves opaque signed
   statement bytes. Index verification downloads actual bytes rather than
   trusting only the index-reported digest.
5. **Privilege and rollout:** Only the two protected upload jobs gain index
   OIDC. They have no checkout or run steps. Only the merge-only dispatcher gains
   Actions write permission; it executes no source code. The enablement variable
   is unset and has not been changed. Existing distribution files remain intact.

## Test self-reviews

1. **Red/green:** Initial ten tests failed against the missing feature; after
   implementation all passed. Added and observed a failing supported-event /
   direct-upload contract before replacing the initial reusable design.
2. **Actual cryptography:** A recorded v0.3.5 GitHub bundle authenticates offline
   with the official library and reaches the expected multiple-subject rejection.
   Corrupt signature, wrong repository, workflow and source digest fail earlier.
   The fixture is public evidence from this repository, not signing credentials.
3. **Boundary doubles:** Atomic staging and index tests double external GitHub /
   index / signature services only. They exercise real wheel validation,
   conversion, policy arguments, signed statement bytes and downloaded digests.
   These are not reported as successful hosted OIDC or PyPI uploads.
4. **Retry and failure:** Tests distinguish HTTP 404 inspection from other API
   failures, reject partial / changed / yanked inventories, missing or duplicate
   predicates and foreign download hosts, and reproduce a valid original-build
   duplicate without overwriting output after a failure.
5. **Breadth and resource bounds:** Run full Python tests, strict Ruff/ty and
   workflow lint, then bounded working-tree mutation verification. Keep mutable
   mutation evidence outside the repository and clean it afterward; record
   actual command results below. Hosted acceptance remains pending configuration.
