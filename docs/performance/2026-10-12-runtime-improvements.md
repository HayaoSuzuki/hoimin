# Runtime performance and maintenance improvements

## Approved scope and execution plan

Baseline: `5b80f08`, Windows, Rust 1.99, Python 3.14. The user approved
implementing #813–#817 on separate work branches after updating main. Preserve
public formats, safety policies, diagnostics, candidate identity and ordering.
Do not add a framework, new input format, or speculative architecture.

1. #813: measure candidate counting, encoding and storage separately. Compare
   the existing two-pass push with a bounded single-pass encoder. Keep discovery's
   allocation-free size check. Reuse JSONL, Unicode and exact-limit tests; verify
   the new writer's refusal before growing beyond its byte budget.
2. #814: measure sequential spool replay. Consider reader ownership only if the
   saving justifies preserving replacement/truncation detection, cursor semantics
   and Windows cleanup. Otherwise retain the current implementation and evidence.
3. #815: measure parsing, depth validation, facts and candidate construction on
   bounded small and real-project inputs. Keep only demonstrated improvements;
   preserve source guards, cancellation and candidate-prefix semantics.
4. #816: move bounded file reading to a neutral internal owner and give shared
   report parsing an owner independent of progress/review command orchestration.
   Keep plan validation separate from run-report validation and do not broaden
   review's accepted formats or impose its 64 MiB limit on progress.
5. #817: centralize version syntax and the existing three platform identities in
   a dependency-free Python policy module. Derive release/SBOM/wheel expectations
   from it while preserving contracts and signature checks in their consumers.

Each task records five design, plan, implementation and verification reviews.
Existing meaningful tests take precedence over artificial failing tests that
only pin private structure. New behavior or boundary responsibilities receive
focused tests before implementation. Final independent review is required.

## Resource and measurement policy

Use one owned Cargo target (`.superpowers/runtime-improvements/target`), at most
two build jobs, no incremental compilation and no debug information for ordinary
checks. Release measurements use the same profile for both versions. Run one
measurement process at a time, bound inputs and repetitions, check disk and process
memory, and clean only the owned target at phase boundaries. Do not duplicate full
build trees. Stop a measurement on failure or timeout rather than report it as a
speedup. Retained probes and raw observations must be sufficient to reproduce
reported numbers; distinguish local costs from plan/run totals.

## Design self-review (five passes)

1. Compatibility: the spool remains compact JSON plus one newline, with identical
   sequence, count, cursor and ID rules. No Arrow or schema migration.
2. Failure handling: reject an oversized encoding before writing; counts advance
   only after successful writes. Keep I/O errors and cleanup behavior observable.
3. Memory: a single record is bounded including its newline; discovery still
   counts without encoding into an unbounded buffer. Avoid caching an entire spool.
4. Evidence: code duplication alone does not establish a hotspot. Local storage,
   analysis and external test time must be reported separately.
5. Boundaries: share format knowledge, not command orchestration. Preserve review's
   historical-source behavior and verify's current-source/fingerprint checks.

## Plan self-review (five passes)

1. Baseline: main fast-forwarded to `5b80f08`; clean branch
   `perf/813-candidate-store` created before retained edits.
2. Dependencies: #813–#815 have a comparison order, not mandatory feature
   dependencies. #816 and #817 can be reviewed independently.
3. Tests: reuse existing corpus/property/boundary checks; add only tests for new
   resource responsibilities. Preserve AAA and avoid conditional test scenarios.
4. Reproducibility: record build profile, input sizes, repetitions, exit status,
   CPU/wall time and process peak memory; separate compile time from runtime.
5. Delivery: commit English evidence with each change, use stacked PRs where
   helpful, retain investigation-only outcomes, and clean owned build artifacts.

## Execution ledger

- Planning: approved in chat; native inline execution, followed by an independent
  final review. No uncommitted user changes were present.
- Resource baseline: approximately 172 GB free on C:. System-wide free-memory
  query is unavailable in the sandbox; do not infer a free-memory value from it.
