# Publishing hoimin wheels and build provenance

`release.yml` builds GitHub Releases and can automatically publish their exact
wheels to TestPyPI and then PyPI. Each wheel carries two attestations: its
original build-time SLSA provenance and a publish attestation. Wheel contents,
platform support, and the 0.3.x version floor are unchanged.

Automatic publication is initially disabled. Leaving `PYPI_AUTO_PUBLISH`
unset lets GitHub Releases continue while the publisher accounts are configured.
The previous `publish-pypi.yml` manual entry point is retired.

## Publication order

1. A PR merged into `main` builds, validates and publishes the GitHub Release.
   `release.yml` signs the existing complete asset inventory and separately signs
   each of the three wheels with a single-subject SLSA statement.
2. When the repository variable `PYPI_AUTO_PUBLISH` equals `true`, an
   `actions: write` job dispatches a publication-only run of `release.yml` on
   `main`. It executes no checked-out code. This explicit dispatch is necessary:
   PyPI rejects upload tokens from `pull_request_target` events.
3. The publication run downloads the public release wheels and checksums,
   verifies metadata/licenses and original build signatures, and converts the
   original bundles to PEP 740 without changing their signed statement bytes.
4. The protected `testpypi` job publishes the three wheels with SLSA and publish
   attestations. A separate read-only job downloads all three wheels and both
   attestations, verifies signatures and identities, pins the build source and
   workflow revisions to the tag commit, and compares SHA256 with GitHub bytes.
5. Only successful TestPyPI verification permits the protected `pypi` upload.
   A final read-only job performs the same checks against production PyPI.

The automatic publisher dispatch is a separate Actions run. Watch both runs;
a successful GitHub Release build alone does not mean PyPI publication passed.
An empty `pypi_tag` retains the existing manual build-preview behavior and
never publishes to either index. PR previews never dispatch PyPI publication.

## One-time publisher setup

The account owner must register a publisher on **both** indexes. Existing
`publish-pypi.yml` registrations do not authorize the new workflow.

- [PyPI hoimin publishing settings](https://pypi.org/manage/project/hoimin/settings/publishing/)
- [TestPyPI hoimin publishing settings](https://test.pypi.org/manage/project/hoimin/settings/publishing/)

| Field | PyPI | TestPyPI |
| --- | --- | --- |
| GitHub owner | `HayaoSuzuki` | `HayaoSuzuki` |
| Repository | `hoimin` | `hoimin` |
| Workflow filename | `release.yml` | `release.yml` |
| Environment | `pypi` | `testpypi` |

Use an ordinary publisher for the existing `hoimin` project, not a pending
publisher for a new project. No API token or repository secret is required.
Both upload jobs are directly in `release.yml`. A reusable upload workflow
would change the OIDC `job_workflow_ref` used to select the publisher and is
therefore deliberately not used.

In [GitHub environments](https://github.com/HayaoSuzuki/hoimin/settings/environments),
retain the `pypi` and `testpypi` environments and restrict their deployment
branches to `main`. Required reviewers cause an approval pause; fully automatic
publication requires these environments to allow upload without a reviewer.
Account registration and environment protection are external settings; merging
this change does not install or enable them.

## Bootstrap and enable automatic publication

Keep `PYPI_AUTO_PUBLISH` unset until setup is complete. Merge this implementation
and wait for a new GitHub Release whose original build includes the three
single-wheel statements. Earlier versions cannot be repaired by resigning their
wheels in a publication job, and existing PyPI files cannot gain attestations
retroactively. Do not use an already-published legacy version for bootstrap.

First publish the new tag only to TestPyPI:

```sh
gh workflow run release.yml --repo HayaoSuzuki/hoimin --ref main \
  -f pypi_tag=vX.Y.Z -f pypi_production=false
```

Review the publication run: all three TestPyPI wheels and both predicates must
verify. Then publish the same bytes to production through the same gate:

```sh
gh workflow run release.yml --repo HayaoSuzuki/hoimin --ref main \
  -f pypi_tag=vX.Y.Z -f pypi_production=true
```

The already-complete TestPyPI publication is verified again instead of uploaded
again. After both index checks pass, remove the obsolete `publish-pypi.yml`
publishers and enable automatic publication for future merged releases:

```sh
gh variable set PYPI_AUTO_PUBLISH --repo HayaoSuzuki/hoimin --body true
```

The variable can also be set in [repository Actions variables](https://github.com/HayaoSuzuki/hoimin/settings/variables/actions).
Delete it or set it to `false` to disable future automatic dispatches. This does
not cancel a publication already dispatched. Manual recovery explicitly opts
into a run and remains available while automatic dispatch is disabled.

## Failures and recovery

Missing or invalid original build evidence, wrong repository/workflow/source,
changed bytes, metadata/license mismatches, and malformed index evidence stop
publication. An authentication failure on TestPyPI prevents any production
upload. A production verification failure marks the publication run failed but
cannot undo files already accepted by PyPI. The GitHub Release remains public.

Before each upload, the index is inspected. A version-level HTTP 404 permits a
new upload. Other HTTP errors fail; they are never interpreted as an empty index.
An existing publication is reused only if its exact three files, downloaded
bytes and both cryptographic attestations verify. Partial publication, legacy
publish-only evidence, wrong digests and duplicate predicates fail. The action
keeps `skip-existing` disabled; it does not silently repair partial uploads.

When no files were accepted, correct the settings and retry the same tag. When
all files were accepted, rerunning verifies them and skips only the now-proven
complete upload. For partial publication, inspect the files and create a new
release version if necessary. Never delete files expecting to reuse filenames.
Multiple valid original signatures from a build rerun are all checked and one
is selected deterministically. No candidate with the matching wheel subject
may fail cryptographic verification.

A workflow/code fix requires a new manual dispatch on updated `main` because
GitHub Re-run retains the original workflow revision. Manual recovery fetches
original build bundles from GitHub's attestation service, so it does not depend
on a seven-day Actions artifact. It neither rebuilds nor relabels wheels.
Concurrent uploads are serialized per index and version; different release
versions do not replace each other's pending uploads. A concurrent duplicate
may fail after another run publishes; recover by verifying the complete upload.

## Verify a downloaded release

Use the pinned tooling from this repository with Python 3.14 and GitHub CLI:

```sh
uv sync --frozen --no-install-project --only-group provenance
uv run --frozen --no-sync python -m tools.pypi_provenance verify \
  --index pypi --tag vX.Y.Z --commit EXPECTED_40_HEX_RELEASE_COMMIT \
  --directory github-release-wheels
```

The directory must contain the three original GitHub wheels. The verifier
retrieves index wheels and evidence through the JSON and Integrity APIs. It
requires `HayaoSuzuki/hoimin`, build workflow `release.yml`, the expected source
and build-workflow commit, a GitHub-hosted builder, and one attestation of each
supported predicate. The expected commit comes from the reviewed release tag,
not from an untrusted statement. Use `--index testpypi` for TestPyPI.
The publish certificate can refer to a later publication-run revision; it is
not treated as the original build source. Provenance does not establish
reproducibility, absence of vulnerabilities, or OS code signing.

## Verification scope and references

Local tests exercise orchestration and real recorded GitHub signature rejection.
They do not establish successful OIDC token exchange or uploads under the new
publisher registrations. Hosted bootstrap evidence remains pending until merge
and account configuration. Record it before treating Issue #771 as complete.

- [PyPI attestation limits](https://docs.pypi.org/attestations/)
- [Bundle conversion and upload](https://docs.pypi.org/attestations/producing-attestations/)
- [Integrity API](https://docs.pypi.org/api/integrity/)
- [Trusted Publisher setup](https://docs.pypi.org/trusted-publishers/adding-a-publisher/)
- [Implementation and self-review](reviews/2026-10-10-issue-771-pypi-provenance.md)
