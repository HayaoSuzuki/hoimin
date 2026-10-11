---
type: Decision
title: Local test improvement by AI agents
description: Product priorities, score interpretation, and resource control for delegated local execution.
status: draft
sources:
- id: principles
  resource: ../product-principles.md
  working_tree: untracked
  sha256: f730f155b1e03a4b0fe84c8491b8e9f13f3ad99148e674713c82d3d6370433cd
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
in force. CI is an optional product use, while hoimin's own development and release
checks remain required. Prioritize useful improvements and clear evidence over
exhaustive runs or score maximization.[^principles]

Read [runtime contracts](design/runtime-lifecycle.md),
[selection and saved plans](design/selection-plan-verify.md), and
[review identity and result separation](design/survivor-review.md) for the existing
mechanisms. This decision adds no execution feature, new guarantee, or Issue-state
change. Recheck it when a proposal introduces score gates, automatic assessment
effects, expanded resource consumption, or distributed execution.

[^principles]: [Product principles](../product-principles.md), the maintainer's direction recorded from the 2026-10-12 discussion.
