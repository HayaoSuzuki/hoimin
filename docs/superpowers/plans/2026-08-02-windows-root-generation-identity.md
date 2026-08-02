# Windows Root Generation Identity Plan

1. Add a failing state-machine regression for two sequential roots sharing one PID.
2. Add a UUID generation ID to supervisors and registered root entries; key exited roots by generation ID.
3. Retain each root process handle until its exit notification is consumed so Windows cannot recycle a PID across unmatched generations.
4. Route classification, detach, termination, drop, and exit notifications through the generation ID, retaining detached tombstones for delayed exits.
5. Confirm active-process-zero notifications against current Job Object accounting before clearing registered generations.
6. Expand focused state tests to prove cleanup of an old generation preserves the new generation, delayed exits consume tombstones first, and stale zero notifications preserve new roots.
7. Run Windows-target checks where available, formatting, clippy, workspace tests, and diff validation.
8. Request independent review, create a PR, wait for Windows and full CI, and merge after success.
