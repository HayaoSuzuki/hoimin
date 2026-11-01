# Analyzer issues 696–708: PR stack and evidence

All thirteen operators are opt-in. The 43 default runtime operator IDs remain unchanged.
Each issue has its own `codex/analyzer-N` branch and `.worktrees/issue-N` checkout.
Designs, plans, five self-review passes per stage (260 total), test/probe records and
Lean models are committed in the corresponding issue branch and inherited by later PRs.

| Issue | Operator | PR | Evidence |
| --- | --- | --- | --- |
| [#696](https://github.com/tokyogas-tech/hoimin/issues/696) | `statement_delete` | [#713](https://github.com/tokyogas-tech/hoimin/pull/713) | [Review](issue-696/review.md) |
| [#697](https://github.com/tokyogas-tech/hoimin/issues/697) | `integer_literal_neighbor` | [#714](https://github.com/tokyogas-tech/hoimin/pull/714) | [Review](issue-697/review.md) |
| [#698](https://github.com/tokyogas-tech/hoimin/issues/698) | `condition_constant` | [#716](https://github.com/tokyogas-tech/hoimin/pull/716) | [Review](issue-698/review.md) |
| [#699](https://github.com/tokyogas-tech/hoimin/issues/699) | `function_body_erase` | [#717](https://github.com/tokyogas-tech/hoimin/pull/717) | [Review](issue-699/review.md) |
| [#700](https://github.com/tokyogas-tech/hoimin/issues/700) | `enum_member_replace` | [#718](https://github.com/tokyogas-tech/hoimin/pull/718) | [Review](issue-700/review.md) |
| [#701](https://github.com/tokyogas-tech/hoimin/issues/701) | `augmented_to_assignment` | [#719](https://github.com/tokyogas-tech/hoimin/pull/719) | [Review](issue-701/review.md) |
| [#702](https://github.com/tokyogas-tech/hoimin/issues/702) | `return_tuple_swap` | [#720](https://github.com/tokyogas-tech/hoimin/pull/720) | [Review](issue-702/review.md) |
| [#703](https://github.com/tokyogas-tech/hoimin/issues/703) | `string_literal_empty` | [#721](https://github.com/tokyogas-tech/hoimin/pull/721) | [Review](issue-703/review.md) |
| [#704](https://github.com/tokyogas-tech/hoimin/issues/704) | `while_condition_false` | [#722](https://github.com/tokyogas-tech/hoimin/pull/722) | [Review](issue-704/review.md) |
| [#705](https://github.com/tokyogas-tech/hoimin/issues/705) | `condition_clause_delete` | [#723](https://github.com/tokyogas-tech/hoimin/pull/723) | [Review](issue-705/review.md) |
| [#706](https://github.com/tokyogas-tech/hoimin/issues/706) | `container_element_delete` | [#724](https://github.com/tokyogas-tech/hoimin/pull/724) | [Review](issue-706/review.md) |
| [#707](https://github.com/tokyogas-tech/hoimin/issues/707) | `conversion_call_remove` | [#725](https://github.com/tokyogas-tech/hoimin/pull/725) | [Review](issue-707/review.md) |
| [#708](https://github.com/tokyogas-tech/hoimin/issues/708) | `optional_keyword_delete` | Current PR (`codex/analyzer-708`) | [Review](issue-708/review.md) |

## Stack integration

GitHub stack **#715** targets `main`. Initial linking used
`gh stack link --base main --open codex/analyzer-696 codex/analyzer-697`.
Each later PR is based on the preceding issue branch and appended with
`gh stack link --open 715 codex/analyzer-N`. This uses the documented remote linking
workflow, which supports independent worktrees without local stack checkout tracking.
The stack is available in the PR interface. No PR has been merged.

## Verification and limits

Every issue has focused public CLI/CPython 3.14 tests, saved-plan weak/strong probes,
analyzer regression checks, a successful workspace test command and fmt/clippy checks.
Issue reviews distinguish full-suite timing from final focused checks after small follow-up
changes. Independent review found and corrected lexical adjacency, identity and resource
issues; exact findings and evidence remain in the per-issue reports.

Each operator has a Lean file under `formal/HoiminOracle/`, checked with `lake env lean`
and an external 20-second limit. These are model-only proofs with stated correspondence
limits, not an end-to-end proof of the Rust implementation or Python parser. See each
review for theorem scope and concrete CPython correspondence tests.

Real-project trials use authored probes, not upstream test suites. Surviving or unexecuted
mutants are not classified as equivalent. Existing `duplicate unary-not operator start`
failures blocked some files; these are recorded as infrastructure failures, not as zero
candidates. The affected pre-existing parser/fact-index bug was not changed in this stack.

## Worktree and disk audit

For #696–#707, `cargo clean` and deletion of issue scratch occurred before creating the
next worktree. Cleanup removed approximately 3.9–4.4 GiB per issue; dependency caches
and pre-existing user files were retained. The final #708 worktree follows the same
cleanup procedure after PR publication. Committed sources, documents and proofs remain
in the isolated worktrees. The user checkout at main was not switched or reset.

At the last pre-publication CI check, PRs #713–#724 had no pending or failed checks;
#725 had only the Windows wheel check pending, with no failures. Final #708 CI starts
after publication and is separate from the local verification recorded in its review.
