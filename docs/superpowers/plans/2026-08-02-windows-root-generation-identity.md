# Windows Root Generation Identity Plan

1. Add a failing state-machine regression for two sequential roots sharing one PID.
2. Add a UUID generation ID to supervisors and active root entries; key exited roots by generation ID.
3. Route attach rollback, classification barriers, unregister, termination, drop, exit notifications, and active-zero notifications through the generation ID.
4. Expand focused state tests to prove cleanup of an old generation preserves the new generation.
5. Run Windows-target checks where available, formatting, clippy, workspace tests, and diff validation.
6. Request independent review, create a PR, wait for Windows and full CI, and merge after success.
