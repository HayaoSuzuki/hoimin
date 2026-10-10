# Issue #771: GitHub CLI download format correction

## Failure and evidence

The first TestPyPI-only dispatch, [run 38064108952](https://github.com/HayaoSuzuki/hoimin/actions/runs/38064108952),
failed in preparation with `KeyError: 'bundle'`. No upload job ran.
The implementation confused the GitHub API's response wrapper with the CLI's
download file format. The initial staging double repeated that incorrect
assumption, so the original green tests did not expose this boundary failure.

Downloaded the public Windows v0.3.6 wheel and ran `gh attestation download`.
Its JSONL has two bare Sigstore objects with top-level `mediaType`,
`verificationMaterial` and `dsseEnvelope`. Saved the actual bytes in
`tests/fixtures/pypi-provenance-download.jsonl`; they contain public certificates,
signatures and statements, not signing credentials.

## Design self-reviews

1. **Boundary:** Confirmed the failing line against the hosted traceback and
   independently reproduced the CLI output locally; this is not an OIDC failure.
2. **Format:** Compared CLI help, captured JSONL and the older API-shaped fixture.
   Parse each line directly as a Sigstore bundle; no invented wrapper fallback.
3. **Trust:** Preserve GitHub source/workflow SHA verification and official
   signature verification. Parsing success never authorizes staging by itself.
4. **Selection:** Actual output includes aggregate and single-wheel statements.
   Keep exact single-subject selection, duplicate verification and atomic output.
5. **Recovery:** The v0.3.6 original build and its proofs remain valid. Dispatch
   on updated main after merge; no rebuild, relabel, overwritten file or resigning.

## Implementation-plan self-reviews

1. **Reproduction:** Change the staging double to bare JSONL first, then observe
   the same missing-wrapper failure before editing production code.
2. **Independent fixture:** Verify the actual single-wheel signature offline
   with repository/workflow and source policies; retain signed statement bytes.
3. **Minimal fix:** Give each unchanged JSONL line to `Bundle.from_json` and save
   that same line for GH verification, removing the wrapper lookup and re-encoding.
4. **Regression coverage:** Re-run atomic staging, missing evidence, digest and
   signature failures, duplicate handling, workflow and index contracts.
5. **Completion:** Strict lint, full Python suite, original-wheel real staging,
   bounded working-tree mutation checks and independent review precede the PR.
   Actual index acceptance still requires the corrected workflow on main.

## Red test result

Before the production fix, the targeted regression command reported
**4 failed, 2 passed, 29 deselected**. All four failures reproduced
`KeyError: 'bundle'`; the real downloaded single-wheel proof authenticated
successfully and the missing-evidence case still rejected input.

## Implementation self-reviews

1. **Minimality:** Removed only the API wrapper lookup and JSON re-encoding.
   `Bundle.from_json(line)` consumes the actual CLI format directly.
2. **Cryptographic boundary:** The original line is written to the bundle file
   supplied to GH verification. Both GH and official library checks still run.
3. **Policies:** Reviewed exact subject/digest, repository, workflow, source SHA,
   signer SHA, predicate type and hosted-runner enforcement; none was relaxed.
4. **Atomicity and reruns:** Output remains deferred until all three signatures
   verify. Duplicate candidates are still all checked before deterministic choice.
5. **Scope and resources:** No workflow, publisher setting, runtime wheel content
   or version-floor change. Temporary downloaded wheels were removed. A review
   worker's permission-limited temporary directory was also removed explicitly.

## Test self-reviews

1. **Red/green:** The corrected raw-JSONL staging double reproduced four hosted
   errors before the fix. All 36 provenance tests passed after it.
2. **Real fixture:** The captured v0.3.6 aggregate and single-wheel bundles are
   decoded by the official library. The single-wheel signature authenticates
   offline with both publisher and expected source policy; signed bytes match.
3. **Mixed responses:** Added aggregate-plus-single staging coverage and checked
   that the bundle passed to GH retains the original signed statement bytes.
4. **Real boundary:** Downloaded all three public v0.3.6 wheels and ran the actual
   staging command with actual GH CLI and official-library signature checks.
   All three wheels and their SLSA files staged successfully; no upload occurred.
5. **Independent and broad checks:** Fresh review found no blockers; its optional
   mixed-response coverage suggestion was included. Strict Ruff and ty passed.
   Full Python and bounded mutation results are recorded separately below;
   they do not replace a successful hosted index upload.

## Validation results

- Provenance regression suite: **36 passed** in 2.57 seconds. Ruff and ty passed.
- Actual three-wheel staging succeeded without external-service doubles.
- Working-tree CLI build succeeded. Bounded mutation run
  `f2ecbb97-83d5-4821-80ed-e63a8fc57347`: **2 killed, 0 survived, 0 errors**.
  Mutations removed the original-bundle write and GH verification call.
  Jobs 1, workspace 8 GiB, minimum free space 10 GiB; peak owned workspace
  39,043,636 bytes and minimum free space 188,685,320,192 bytes. Temporary
  manifest/report directory and workers were cleaned.
- Final `cargo clean` removed 2,580 files / 2.2 GiB; free space was
  190,792,257,536 bytes.
- First full Python run: **896 passed, 28 skipped, 1 failed**, 322.61 seconds.
  The failure was the unchanged Lean workflow contract test's 60-second Git
  Bash subprocess limit. Targeted invocation also timed out. Its log reached
  all 297 fake Python calls. A diagnostic run with a temporary 120-second limit
  passed every original assertion in 55.10 seconds; this is diagnostic evidence,
  not a normal-suite pass. The checked-in timeout and test remain unchanged.
- A final full run without compilation or parallel tests is being checked.
  Index bootstrap remains pending corrected main.
