# Candidate spool lifetime design

## Problem

`CandidateStore::finish` must preserve its temporary file so workers can replay
it, but the file is currently created in the process-wide temporary directory.
No run component owns the preserved path, so every completed, failed, or
cancelled run leaves the candidate JSONL behind.

## Design

Allow `CandidateStore` to create its named file inside a caller-provided
directory. `AnalyzerHandler` gains an optional candidate spool directory; the
normal shell construction supplies the root of its existing run-scoped
`TempDir`. The directory owner is reference-counted and cloned into blocking
analysis tasks, so cancellation can return promptly without deleting the
directory while a detached task still has the spool open. The preserved file
remains available throughout machine execution and is recursively removed
after both `ShellContext` and any detached analysis have released ownership.

Standalone analyzer construction keeps the existing process-temp behavior so
its public API and focused tests do not require a separate directory owner.
No spool token is added to cleanup effects or session persistence: resume
reanalyzes, and context ownership already covers every shell exit path.

## Tests

Store tests will prove that a finished spool is created beneath the requested
directory and disappears when that owning temporary directory is dropped.
The in-progress cancellation test will also prove that prompt cancellation
retains the directory until detached blocking analysis exits, then removes it.
Existing analyzer replay, run, and shell tests continue to cover availability
during execution.
