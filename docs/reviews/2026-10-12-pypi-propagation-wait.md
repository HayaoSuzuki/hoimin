# PyPI propagation waiting

## Evidence and change

Run [38150317127](https://github.com/HayaoSuzuki/hoimin/actions/runs/38150317127)
failed verifying TestPyPI v0.4.2 after successful upload: version JSON returned
404 throughout the original six attempts (about 50 seconds of waiting).
Earlier v0.4.0 and v0.4.1 publication runs showed the same symptom and recovered
when rerun after propagation. This is evidence for increasing a bounded wait,
not evidence of a guaranteed maximum index propagation delay.

Post-upload verification now retries only HTTP 404, with a 600-second monotonic
retry budget, 10-second intervals and at most 61 attempts. It checks the deadline
after sleeping before starting another attempt. Network/verification time counts
toward the retry budget, but an already-running verification may finish later.
Both verification jobs have a 15-minute hard limit including dependency setup.
Inspection remains immediate and the byte/inventory/signature policies are intact.

The implementation is on `fix/pypi-propagation-wait`, independently of #749.
There are no changes to distributed binaries or their version.
Existing runs retain their original code on rerun; a new dispatch on merged main
is needed to exercise the updated waiting code.

## Design self-review (five passes)

1. Confirmed failure occurs after upload, at version JSON lookup; credential
   setup or relaxed artifact verification would not address this failure.
2. Kept 404 retrying limited to verify mode; inspect still distinguishes a new
   upload immediately and does not treat permission/server errors as absence.
3. Defined both retry and job limits; did not claim a hard 600-second bound for
   an in-flight multi-request verification.
4. Preserved complete inventory, wheel hash, publisher and both attestations.
5. Kept mitigation independent of documentation publication and package versions.

## Planning self-review (five passes)

1. Selected the existing provenance verifier and its two jobs as the only runtime
   changes; did not add another workflow or publisher.
2. Planned generated delayed-success cases covering more than the old six attempts.
3. Included count exhaustion and time already consumed by HTTP requests.
4. Included immediate failure for non-404 HTTP errors and invalid publication.
5. Included operational documentation, OKF provenance and a fresh code review.

## Implementation self-review (five passes)

1. Extracted waiting into a helper called by the CLI; inspect keeps allow_missing.
2. Checked monotonic deadline arithmetic and clamped the final sleep.
3. Checked retry progress identifies index/version without tokens or response data.
4. Reviewed exact workflow diff: only the two verification job limits increase.
5. Reviewed the full test diff and restored an existing inventory test accidentally
   affected by context-manager formatting; unrelated tests remain unchanged.

## Verification self-review (five passes)

1. Watched six new tests fail before helper implementation, then pass; generated
   inputs cover zero through 60 delayed 404s without real sleeps or network calls.
2. Verified exhaustion rethrows the HTTP error and never reports publication success.
3. Fresh review identified retry starting at an expired deadline. A simulated
   clock reproduced two calls instead of one (RED); the deadline recheck makes
   that boundary test pass (GREEN).
4. Checked non-404 errors, invalid publication and immediate inspection behavior,
   plus the existing recorded-signature and publication-contract test suite.
5. Ran strict Ruff, ty and workflow lint. Workflow lint reports no findings; its
   pre-existing unsupported Python-shell warnings/three configured exclusions
   remain. The real v0.4.2 run succeeded on rerun using the old code, which is
   recovery evidence rather than live validation of this new mitigation.

## Independent review

A fresh reviewer found the accidental existing-test edit and the deadline boundary
above. Both were fixed; the deadline fix was validated RED-to-GREEN. No weakening
of security validation was found. Full related-test results are recorded in the PR.
