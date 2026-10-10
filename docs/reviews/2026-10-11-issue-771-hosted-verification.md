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
- Working branch for this evidence: `docs/issue-771-hosted-verification`.

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
No successful index upload or OIDC exchange is claimed. Automatic dispatch
remains disabled; the corrected workflow must reach main before a new dispatch.
