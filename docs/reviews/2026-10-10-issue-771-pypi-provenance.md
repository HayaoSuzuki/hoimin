# Issue #771: PyPI build provenance and automatic publication

## Approved design

After a merged main-branch release, `release.yml` publishes the GitHub Release,
publishes its exact wheels to TestPyPI, retrieves and verifies all three wheels
and both attestations, and only then publishes to PyPI and verifies it too.
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
2. **Trust boundary:** Cryptographic verification precedes interpretation and
   staging. Downloaded files are data; upload jobs execute no release code.
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
- pypi-attestations `0838c9fe75b77ef1ee9053cc73ef91454c11b453`,
  `src/pypi_attestations/_impl.py`: exactly one subject; Build Config URI policy.
- PyPA publishing action `dc37677b2e1c63e2034f94d8a5b11f265b73ba33`:
  generates `*.publish.attestation`, refuses existing publish statements, and
  uploads adjacent attestations through Twine.

## Validation results

Implementation and test reviews will be recorded with actual results.
