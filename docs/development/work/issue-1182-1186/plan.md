# Contract and amplification guidance implementation plan

> Execute inline with the executing-plans workflow; the existing checkout is an
> isolated worktree. Use writing-skills application scenarios to validate guidance.

**Goal:** Ship the #1182/#1186 guidance and a separate seven-issue research summary.

**Architecture:** The skill owns execution policy, a bundled reference owns test
design examples, and the knowledge page owns research conclusions and provenance.

**Tech stack:** Markdown, existing pytest packaging checks, gh skill dry-run,
existing strict MkDocs publication tool.

**Spec:** [design.md](design.md).

## Constraints and review focus

- Preserve current resource, cleanup, identity and progress contracts.
- Use English for maintained docs; preserve raw historical evidence.
- No new production functionality or tests that merely mirror prose.
- New executable test cases, if needed, use one case per invocation,
  parameterization/property-based input generation and AAA.
- Do not infer full-suite gaps from focused survivors or human acceptance from kills.
- Replan for changed argv; a common candidate set alone does not erase context changes.
- Do not label correct caller isolation as mock drift; cover allowed implementation differences.
- Single-skill installs must resolve the new bundled reference.
- Research summary must distinguish controlled toys, real functions and independent bugs.

## Tasks

- [x] Run application scenarios before editing the skill; preserve responses and
  note whether the baseline already makes correct decisions.
- [x] Add `skills/hoimin-mutation-improvement/test-design.md`; link it from the
  diagnostic and reporting portions of SKILL.md without changing guarded blocks.
- [x] Add the user-facing guide link in `docs/user/guides/agent-skills.md`.
- [x] Add `docs/development/knowledge/testing-research-evaluation.md`, linked from
  knowledge and development indexes; cover both evaluation commits.
- [x] Run updated-skill application scenarios and manually review their decisions.
- [x] Run existing skill tests, packaging dry-run, strict docs build and diff checks.
  Check bundled links and evidence totals. Add a targeted test only if a behavior
  defect outside prose requires a regression check.
- [x] Finish five implementation and five test review passes; record design/docs scores.
- [ ] Commit, push the branch including both research commits, create a PR, and
  inspect required CI. Keep unresolved research conditions visible in the PR.

## Plan self-review: five passes

1. Sequence: inspect unchanged evidence before drafting conclusions; baseline
   scenarios precede skill edits; verify actual installed content before publishing.
2. Test value: use existing packaging/link guards and application scenarios, not
   new phrase-match assertions that only restate the implementation.
3. Reproducibility: list commands and observed results; no rerun of unrelated Rust
   mutation suites just to support a docs-only change.
4. Scope preservation: include two requested research commits in the PR without
   rewriting their original observations; add an independently readable summary.
5. Completion: publishing a PR is authorized. Do not auto-close #1182/#1186 while
   their empirical adoption/cross-project tasks remain unfulfilled.
