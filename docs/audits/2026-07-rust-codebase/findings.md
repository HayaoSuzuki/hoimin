# Findings Ledger

## Status vocabulary

- `lead`: requires semantic review
- `accepted`: actionable root cause
- `rejected`: not actionable, with rationale
- `issue_created`: accepted and linked to GitHub

Classification accepts only `confirmed bug`, `high-risk design`, or `maintainability`.
Severity accepts only `P0`, `P1`, `P2`, or `P3`.

| ID | Area | Classification | Severity | Status | Locations | Evidence | Root cause / boundary | Disposition |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| RUST-001 | isolation | high-risk design | P1 | lead | `crates/hoimin-cli/src/process/mod.rs:341`; `crates/hoimin-cli/src/resource/portable.rs:106-121` | `Command::spawn` returns a running child before `PortableSupervisor::attach` assigns it to the kill-on-close Windows Job Object. The source itself documents that the child can execute in this interval. | On Windows portable mode, process creation is not suspended or otherwise atomic with Job Object assignment. A fast child can perform work or spawn descendants before the process tree is placed under the configured lifetime/resource boundary; attach failure only kills the root handle and cannot prove pre-assignment descendants are contained. | Confirm with a Windows stress test using a child that immediately spawns a detached descendant, then design suspended startup/assignment before resuming. |
