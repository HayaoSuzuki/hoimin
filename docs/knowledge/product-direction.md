---
type: Decision
title: Local test improvement by AI agents
description: Product priorities, score interpretation, and resource control for delegated local execution.
status: draft
sources:
- id: principles
  resource: ../product-principles.md
  revision: 7c2a79168c8698f85cc90d7efd6bf16a183e08b3
  working_tree: modified
  sha256: 1c51723e2f4209c87ba987efbe18bac739d898377086fcd059b724ccc836093e
---

# Local test improvement by AI agents

The maintainer clarified the product direction on 2026-10-12: an external AI
agent uses hoimin locally to investigate selected survivors and improve tests
within an explicit resource budget. Humans resolve specification ambiguities and
review changes. The canonical [product principles](../product-principles.md)
record that decision; it is not evidence that hoimin includes an LLM or that
agent-generated tests are automatically correct.[^principles]

Mutation scores support diagnosis, not a standalone quality target. Equivalent
and specification-permitted changes complicate interpretation; sampling intervals
cannot resolve that problem. Assessments remain distinct from execution results,
and suspected equivalence is not a proof.[^principles]

Resource limits, termination, cleanup, and honest partial results are central to
delegated local execution. Their existing backend and aggregation limits remain
in force. The 2026-10-23 clarification places users' CI integration outside the
product scope and removes mutation-score and regression gates. Complete run/verify
executions and valid progress comparisons return success regardless of survivors
or regressions. Agents inspect statuses, scores and histories to select their next
steps. hoimin's own development and release checks remain required. Prioritize
useful improvements and clear evidence over exhaustive runs or score
maximization.[^principles]

The 2026-10-28 clarification makes opt-out the normal policy for newly adopted
mutation operators: include accepted operators in the default runtime set and
allow `--exclude-operators`. Adoption still requires evaluation; an experimental
proposal is not enabled merely because it exists. See the
[slice, division and selection research](mutation-research-1121-1122-1124.md)
for a scoped evaluation that distinguishes adoption recommendations from
implemented behavior.[^principles]

Read [runtime contracts](design/runtime-lifecycle.md),
[selection and saved plans](design/selection-plan-verify.md), and
[review identity and result separation](design/survivor-review.md) for the existing
mechanisms. This decision adds no execution feature, new guarantee, or Issue-state
change. Recheck it when a proposal introduces score gates, automatic assessment
effects, expanded resource consumption, or distributed execution.

[^principles]: [Product principles](../product-principles.md), the maintainer's direction recorded on 2026-10-12 and clarified on 2026-10-23.
