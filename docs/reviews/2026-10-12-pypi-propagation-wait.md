# PyPI propagation waiting

## Evidence and change

Run [38150317127](https://github.com/HayaoSuzuki/hoimin/actions/runs/38150317127)
failed verifying TestPyPI v0.4.2 after successful upload: version JSON returned
404 throughout the original six attempts (about 50 seconds of waiting).
Earlier v0.4.0 and v0.4.1 publication runs showed the same symptom and recovered
when rerun after propagation. This is evidence for increasing a bounded wait,
not evidence of a guaranteed maximum index propagation delay.

Post-upload verification now retries only HTTP 404, with a 600-second monotonic
retry budget, exponential backoff (10, 20, 40 seconds, then capped at 60 seconds)
and a defensive cap of 61 attempts. It checks the deadline
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
   inputs cover zero through 11 delayed 404s with a simulated clock, without real
   sleeps or network calls. A separate frozen-clock test covers the defensive
   attempt limit.
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

## Exponential-backoff follow-up

Design self-review (five passes): checked the requested doubling schedule;
selected a 60-second cap; retained the monotonic budget; retained 404-only retry;
kept inspect and signature validation unchanged.

Planning self-review (five passes): added a schedule regression test first;
planned a clock that advances with sleep; covered a shortened final sleep;
checked the CI dependency groups after the reported failure; kept Hypothesis in
normal development dependencies without adding Hypothesmith to ordinary CI.

Implementation self-review (five passes): checked exponent indexing starts at
zero; checked the interval cap; checked remaining-budget clamping; checked
the deadline recheck prevents a thirteenth request at 600 seconds; checked the
lock diff adds only the existing Hypothesis dependency to the dev group.

Verification self-review (five passes): the schedule regression failed with
fixed intervals before the change; the simulated clock covers delayed success;
exhaustion performs 12 requests and sleeps exactly 600 seconds; a fresh default
dev environment without the fuzz group passes all 48 provenance tests; strict
Ruff and ty checks pass. CI run 38153505968 failed collecting these tests because
Hypothesis was previously present only in the fuzz group. That dependency error
is corrected in this follow-up; hosted CI must confirm the updated commit.
