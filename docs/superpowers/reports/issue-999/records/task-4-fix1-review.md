- **Valid additional preconditions bypass complete source-contract checking** — ADDRESSED (`tools/verify_creusot.py:183-201`). The runner now compares the complete multiset of attached outer attributes with the manifest's exact approved attributes, exempting only `#[must_use]`. Alternative `cfg_attr` predicates, qualified/nested contracts and extra attributes therefore fail; the new tests cover those cases plus intervening comments and same-line attributes (`tests/test_verify_creusot.py:74-140`).
- **SIGTERM leaves the command session alive** — ADDRESSED (`tools/verify_creusot.py:397-449`). A scoped SIGTERM handler raises `ProofError` into the existing owned-session cleanup, then restores the prior handler. The new Linux subprocess test checks that a separately grouped child has stopped and the wrapper exits through `ProofError` (`tests/test_verify_creusot.py:374-432`); the report also records the corresponding actual Linux result (exit 17, child state Z).

### New Breakage in the Fix Diff

- **Important — launched proof tools inherit blocked SIGTERM on Linux** (`tools/verify_creusot.py:411-434`). `pthread_sigmask(SIG_BLOCK, {SIGTERM})` runs before `Popen`, and the parent restores the mask only after `Popen` returns. POSIX children inherit their parent's mask, so `/usr/bin/time`, Cargo, compiler and solver descendants start with SIGTERM blocked and do not respond to that signal even after the runner unmasks itself. A focused subprocess check using the same `pthread_sigmask`/`Popen(start_new_session=True)` sequence printed `child_SIGTERM_blocked=1`. The existing child-termination test sends SIGTERM only to the runner and observes its subsequent SIGKILL cleanup, so it does not detect this regression. Keep SIGTERM unblocked in the child while closing the launch race, for example with a temporary handler that records the pending stop until `Popen` returns.

### Out-of-Scope Observations

- The previously reported relative-source-path diagnostic warning remains deferred to Task 5/final review; it is unchanged by this fix.

### Checks

- Read the fix diff once, the prior review, Task 4 brief/constraints and implementer report. The report names focused host results (`37 passed, 2 skipped`), a Linux SIGTERM child result (exit 17/state Z), and an actual Python 3.12.3 three-target fresh proof run (`34.459s`, 25 Coma files/59 goals/25 strategies per target); these claims align with the changed code and saved summary. Native x86_64 after-fix CI remains for the controller.
- Ran one focused POSIX signal-mask inheritance check for the concrete launch doubt; child reported SIGTERM blocked. Did not rerun passing suites or full proofs. Docker access was denied, so this check did not execute inside the Linux proof container.

### Verdict

**Fix round:** Findings remain open — both original Important findings are ADDRESSED, but the fix introduces an Important SIGTERM-mask inheritance regression at `tools/verify_creusot.py:411-434`.
