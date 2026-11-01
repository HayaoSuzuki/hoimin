### Spec Compliance

- ✅ Spec compliant for Task 1 / #974, including the controller-approved cleanup scope. The private descriptor guard rejects shared targets before chmod and rechecks the final writable descriptor (`crates/hoimin-cli/src/workspace/root.rs:174`, `root.rs:210`). Unix queries descriptor metadata; Windows queries `GetFileInformationByHandle` (`root.rs:216`, `root/windows.rs:81`). No automatic detachment or compatibility behavior was added.
- ✅ Explicit path-bearing error, diagnostic code, and effect conversion are present (`crates/hoimin-cli/src/workspace/mod.rs:137`, `mod.rs:198`, `mod.rs:1567`). All four planned source areas have corresponding changes; `reset.rs` needs no change because rejection belongs at mutation conversion.
- ✅ Shared readonly cleanup avoids chmod while retaining existing single-link behavior (`crates/hoimin-cli/src/workspace/root.rs:527`, `root/windows.rs:117`, `root/windows.rs:195`). This is within the explicit controller ruling.
- ⚠️ Native Windows and Linux execution are not established by the supplied evidence. Windows safe readonly unlink may fail, as explicitly permitted; native CI should exercise the common regressions and `root/windows.rs:943`. This is a disclosed platform-validation limit, not an implementation finding.

### Strengths

- The guard sits at the existing permission/write boundary. The retained Unix parent, no-follow open, dev/ino comparison, and Windows identity-reopen call remain intact (`crates/hoimin-cli/src/workspace/root.rs:173`). Candidate validation code is unchanged, and the supplied final logs include hash/span/original and readonly-rejection controls.
- Separate named tests exercise original and sentinel referents immediately after baseline-equivalent replacement and after reset; the readonly scenario additionally detects permission side effects (`crates/hoimin-cli/src/workspace/mutation.rs:354`). Assertions compare literal external bytes, captured permissions, the typed error/path/code, and external state through cleanup/drop (`mutation.rs:294`). The bounded property adds 1–8 aliases over 16 cases (`mutation.rs:413`).
- Cleanup coverage includes public removal and Windows guarded removal. Setup permissions are restored only after invariant assertions, so teardown does not conceal permission corruption (`crates/hoimin-cli/src/workspace/mutation.rs:379`, `root/windows.rs:943`).

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- None found.

#### Minor (Nice to Have)

- None found.

### Review Checks and Scope

1. Acceptance pass: matched authoritative issue acceptance criteria and the brief to the supplied four-file diff, including direct/reset original/sentinel cases, explicit rejection, no detachment, and readonly preservation (`mutation.rs:354`, `root.rs:174`). Final documentation remains Task 10 as planned.
2. Mutation-boundary pass: checked descriptor-based counts, pre-chmod ordering, final-descriptor validation, error propagation, and the preserved identity check (`root.rs:173`, `root.rs:216`, `root/windows.rs:81`). The diff cuts `into_writable` mid-function, so the only supplemental production read was `root.rs:174–217` to inspect the omitted retained-parent/open/identity sequence. No broader source inspection or diff rederivation was performed.
3. Cleanup pass: checked Unix public removal and both Windows removal variants for shared-inode chmod suppression and preserved single-link handling (`root.rs:497`, `root/windows.rs:111`, `root/windows.rs:188`). Existing deletion implementation, recursive Unix cleanup, and other unchanged write APIs were not independently audited.
4. Test/evidence pass: read the preserved initial RED and cleanup RED logs, plus both final contracts logs. Final evidence records 12 unit tests passed / 1 pre-existing manual benchmark ignored and 40 integration tests passed, with no warnings in those final logs. Initial RED is explicitly a condensed transcription; cleanup RED and final logs are direct captures. No tests or Cargo commands were rerun. Four successful format/clippy hooks are controller-supplied evidence and recorded in `task-1-report.md:99`; they were not independently rerun.
5. Report/quality pass: compared the report's claims against the code and logs. The five passes recorded for requirements, implementation, verification, and completion are substantive and distinct, and the cleanup expansion has its own five design and implementation checks (`task-1-report.md:7`, `:23`, `:31`, `:75`, `:83`). The report correctly limits platform claims (`task-1-report.md:81`, `:108`). The small production helper centralizes the mutation invariant without unrelated restructuring.

### Assessment

**Task quality:** Approved.

**Reasoning:** The implementation closes the demonstrated shared-inode mutation and permission side effects at the descriptor boundary, preserves existing identity handling, and supplies behavioral regressions with passing focused macOS evidence. No task-blocking defect was found; native platform validation and the final integrated suite remain with the controller/CI.
