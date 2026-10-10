# Issue #771 hosted bootstrap verification

## Scope and source

This records hosted verification after PR #773, rather than extending the local
test results into a claim about PyPI. The implementation's five self-reviews
for each of design, plan, implementation and testing remain recorded in
[the implementation review](2026-10-10-issue-771-pypi-provenance.md).

The account owner reported registering `release.yml` Trusted Publishers on
both indexes. GitHub's `pypi` and `testpypi` environments were independently
queried: each restricts deployment to `main`, with no required reviewer.
`PYPI_AUTO_PUBLISH` was absent before bootstrap.

- Merge commit: `c9f92eb721d8a858a2f55643a5c72723763174dd`.
- Release tag: `v0.3.6`; GitHub's tag API resolves to that merge commit.
- Original build: [run 38063176629](https://github.com/HayaoSuzuki/hoimin/actions/runs/38063176629).
- Final evidence branch: `docs/issue-771-bootstrap-results`.

## Operational plan review

1. Confirm the merge, tag and original build identity before requesting uploads.
   PR preview assets and earlier releases are not bootstrap inputs.
2. Keep automatic dispatch disabled until both index uploads and downloaded
   evidence pass. Registration alone does not establish Warehouse acceptance.
3. Start a TestPyPI-only dispatch on main. Its upload must carry the original
   single-wheel SLSA statement and a separately generated Publish statement.
4. After TestPyPI verification, dispatch production for the same tag. The
   existing TestPyPI files must verify before reuse; do not use `skip-existing`
   or overwrite accepted files.
5. Independently retrieve the public index files and proofs, compare GitHub
   checksum bytes, verify identity and rejection cases, then enable future
   automatic dispatch and commit the public evidence.

## Hosted results

The original build and GitHub Release succeeded. The first TestPyPI-only run,
[38064108952](https://github.com/HayaoSuzuki/hoimin/actions/runs/38064108952),
failed before uploads because preparation expected an API wrapper in the CLI's
JSONL. See [the format correction review](2026-10-11-issue-771-download-format.md).
PR #774 merged as `3dbf6dfdb107cb83c1834fa8d3bf057e95243326` and corrected
the CLI boundary without rebuilding or resigning v0.3.6.

| Phase | Run | Result |
| --- | --- | --- |
| TestPyPI-only publication and downloaded verification | [38066702628](https://github.com/HayaoSuzuki/hoimin/actions/runs/38066702628) | Success |
| TestPyPI reuse, PyPI publication and downloaded verification | [38066847354](https://github.com/HayaoSuzuki/hoimin/actions/runs/38066847354) | Success |

The second run verified the complete TestPyPI publication and skipped its
upload steps before uploading to production. Both indexes accepted the new
Trusted Publisher identity and both attestation predicates.

An independent local invocation downloaded the original release and staged
all three original signatures, then used `verify_index` against both public
indexes. Actual downloaded bytes and both signatures passed. Per-wheel index
SHA256 matched GitHub wheels and `SHA256SUMS`; converted SLSA statement bytes
matched the original GitHub statement exactly. Build source and signer digest
were both `c9f92eb721d8a858a2f55643a5c72723763174dd`, while the publication run
used the later correction revision. These identities were kept distinct.

Five GH verification cases rejected modified bytes, a foreign repository,
wrong source commit, wrong workflow and corrupt signature. Public publisher
attributes, certificate attributes, checksums, statement hashes and rejection
results are recorded in the [machine-readable evidence](2026-10-11-issue-771-hosted-verification.json).
All temporary downloaded wheels and bundles were removed.

After both hosted runs and independent verification succeeded, set
`PYPI_AUTO_PUBLISH=true` and read it back with `gh variable get`. This enables
dispatches for future merged releases; it does not retroactively publish old
versions or prove that a not-yet-observed automatic dispatch succeeded.

## Execution self-reviews

1. **Main and tag:** Recovery used updated main but fetched v0.3.6's original
   tag and build evidence. The source check remained pinned to the original SHA.
2. **Publisher:** TestPyPI and PyPI uploads actually succeeded under `release.yml`
   with the configured environment; registration was not inferred from a report.
3. **Order:** TestPyPI verification completed before requesting production.
   Existing TestPyPI files were verified and reused, rather than overwritten.
4. **Enablement:** Automatic dispatch stayed disabled through bootstrap and was
   enabled only after hosted and independent checks passed; the value was read back.
5. **Evidence and scope:** Stored public attestations' attributes and hashes,
   not credentials or binary wheels. Earlier local limitations and the first
   failed dispatch remain documented rather than replaced by a success claim.

## Verification self-reviews

1. **Inventory:** Both indexes expose exactly the three expected wheels; each
   actual download matches the GitHub wheel and original checksum entry.
2. **Proofs:** Every wheel carries exactly one SLSA and one Publish predicate;
   official signature verification and GH build-policy verification passed.
3. **Original bytes:** Each index SLSA statement matches the converted original
   GitHub statement byte-for-byte; publication did not invent build evidence.
4. **Identity and rejection:** Source/workflow digests and hosted-runner policy
   were enforced. Five real-bundle negative cases were all rejected.
5. **Retry and limits:** Production's second pass demonstrated complete TestPyPI
   reuse. Partial publication repair remains refused by tested code; it was not
   forced on the live index. Provenance still does not prove reproducibility,
   absence of vulnerabilities or OS code signing.

## Following release v0.3.7

PR #774 produced [build run 38066658469](https://github.com/HayaoSuzuki/hoimin/actions/runs/38066658469)
and the v0.3.7 GitHub Release at `3dbf6dfdb107cb83c1834fa8d3bf057e95243326`.
That run started before automatic publishing was enabled; its dispatch job
was skipped. Started publication explicitly in
[run 38067591690](https://github.com/HayaoSuzuki/hoimin/actions/runs/38067591690).
TestPyPI upload and downloaded verification succeeded, followed by PyPI upload.
The first PyPI verification received HTTP 404 from the version JSON endpoint.
After independently confirming that endpoint exposed version 0.3.7 and three
files, reran only failed jobs. Attempt 2 succeeded, including downloaded bytes
and both attestations. No second upload or overwrite was performed.

This release has hosted verification; the independent local six-wheel evidence
and five negative cases above refer specifically to v0.3.6. Automatic dispatch
is configured for future merges, but this bootstrap did not observe a successful
merge-triggered dispatch. Index propagation can still require a failed-verification
rerun; automatic recovery from that transient is not currently implemented.

Documentation checks on 2026-10-11 JST passed: OKF YAML/reserved-file checks for
35 Markdown files, changed source hashes and footnotes, local links and evidence
consistency. This final branch changes documentation only; earlier Python tests
are not represented as rerun here. `cargo clean` removed zero files and the
workspace drive retained approximately 191 GB free.
