# Issue 259 Complete Plan Ranking Design

## Problem

Plan ranking requires every candidate to contain exactly one operator-derived
reason. The operator classifier predates collection, structural, bitwise, and
exception mutations and returns no reason for those variants. `hoimin plan`
still serializes the result, while `hoimin verify` applies the invariant and
rejects the same manifest.

This is both an incomplete classification and an output-boundary defect: plan
creation can return a document that the plan reader considers invalid.

## Goals

- Assign exactly one fixed ranking reason to every current mutation operator.
- Make future enum additions fail compilation until their ranking category is
  chosen.
- Preserve deterministic ranking and make the new scoring semantics explicit
  through the ranking rule version.
- Validate every generated manifest before returning it from `create`.
- Cover plan-to-verify interoperability for the newly added operator families.

## Non-goals

- Changing candidate discovery, operator defaults, or selector expansion.
- Introducing data-dependent ranking or runtime test feedback.
- Supporting manifests produced with an older ranking rule version in current
  verification; plans remain reproducible artifacts tied to their rule version.
- Retuning selector bonuses or the existing category scores.

## Considered approaches

### Recommended: add semantic categories and make classification exhaustive

Add `exception_handling` at 90 points and `behavioral` at 80 points. Exception
operators belong to the former; collection and structural operators belong to
the latter; bitwise operators join the existing 70-point arithmetic category.
The original high-value-control (100) and type-annotation (50) categories stay
unchanged.

This keeps the ranking explanation meaningful instead of calling collection
shape changes “control flow.” The ranking rule version advances from 2 to 3
because serialized reason codes and candidate scores change.

### Alternative: map every new operator to existing categories

This would minimize schema surface, but classifying collection and structural
behavior as high-value control is misleading to users and future maintainers.
It would still change ranking scores and require a rule-version bump.

### Alternative: introduce one generic fallback category

A catch-all reason would make manifests valid but hide missing design choices
for future operators. It would recreate the root cause in a less visible form
and prevent the compiler from enforcing completeness.

## Classification

- `high_value_control` (100): comparison, membership, identity, boolean,
  `not`, boolean literal, and break/continue operators.
- `exception_handling` (90): all safe and risky exception handler operators.
- `behavioral` (80): collection-call/literal and structural behavior operators.
- `arithmetic` (70): arithmetic, unary sign, and bitwise operators.
- `type_annotation` (50): all type-annotation operators.

Selector reasons remain ordered ahead of operator reasons:
`explicit_line`, `explicit_symbol`, `changed_line`, then exactly one category
from the list above. Scores keep descending with enum order, preserving the
existing validation rule for sorted, unique reasons.

## Architecture

`operator_reason` continues to parse the canonical string into
`MutationOperator`, but its match becomes exhaustive and returns a category for
all 43 variants. Only an unknown string returns `None`; analyzer-produced
candidates always use canonical IDs.

`RankingReasonCode`, `fixed_score`, and `is_operator_reason` gain the two new
categories. `RANKING_RULE_VERSION` becomes 3.

After `create` builds `PlanManifest`, it calls the existing `validate_header`
before constructing `PlanOutput`. This reuses the exact schema, record,
ranking, candidate-ID, and normalized-path validation applied by verification.
An internal invariant failure is returned as the existing typed `PlanError`
instead of being serialized.

## Error handling and compatibility

Unknown operator strings still receive no reason and are rejected by manifest
validation. This preserves validation at untrusted input boundaries while the
exhaustive enum match protects internal canonical operators.

Rule-version-2 manifests remain structurally readable but are rejected with the
existing unsupported-ranking-version diagnostic. Users regenerate plans with
the current binary, matching the established compatibility contract.

No plan schema version or report schema changes are required.

## Testing

- A table-driven ranking test enumerates all canonical operator IDs, asserts
  the expected category and score, and validates the resulting ranking.
- Representative assertions distinguish exception, behavioral, arithmetic,
  control, and type categories.
- A test-only malformed canonical candidate proves `create`-boundary validation
  cannot emit a missing-reason manifest without relying on serialization.
- Plan integration fixtures cover collection, structural, bitwise, and
  exception selections and pass their emitted manifests through
  `prepare_verify_selection`.
- Existing tamper, stable-order, plan JSON, and rule-version tests are updated
  for version 3.

Formatting, Clippy, the Rust workspace suite, and Python contract tests remain
the merge gate.
