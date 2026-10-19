- **Launched proof tools inherit blocked SIGTERM on Linux** — ADDRESSED (`tools/verify_creusot.py:395-428`). The runner no longer calls `pthread_sigmask`; it records SIGTERM while `Popen` assigns the owned PID, then raises into session cleanup. After launch, the handler raises immediately. The Linux child-delivery test (`tests/test_verify_creusot.py:430-484`) sends SIGTERM directly to a live, separately grouped child and requires its handler marker and a successful wrapper exit.

### New Breakage in the Fix Diff

None.

### Out-of-Scope Observations

- The previously reported relative-source-path diagnostic warning remains for Task 5/final review; this diff did not touch it.

### Checks

- Read the supplied fix diff once, the prior review, Task 4 brief and constraints, the appended implementation report, and the changed runner/test lines. The added launch and reap transition tests (`tests/test_verify_creusot.py:487-521`) match the handler and cleanup conditions (`tools/verify_creusot.py:399-440`). No new Critical or Important breakage was found in the fix diff.
- The implementation report records actual Linux child signal delivery (`wrapper=0`, handler marker present), parent SIGTERM cleanup (`exit=17`, child state `Z`), and focused host tests (`39 passed, 3 skipped`). The saved Python 3.12.3 run summary reports `passed` in 35.851 seconds, with 25 generated Coma files, 59 proved goals and 25 strategies on each of the three targets. These covering results are consistent with the changed code; I did not repeat the suite or full proofs.
- Native Linux x86_64 CI against the final runner remains the controller's integration check.

### Verdict

**Fix round:** All findings addressed, no new Critical/Important breakage.
