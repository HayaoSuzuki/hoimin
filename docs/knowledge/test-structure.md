---
type: Playbook
title: AAA test structure
description: Arrange fixtures, execute the subject and assert related outcomes in Python and Rust.
status: draft
catalog_revision: a7d94ab3d08877d9f615122ca8b5deccb1042ceb
sources:
- id: development
  resource: ../development.md
  revision: a7d94ab3d08877d9f615122ca8b5deccb1042ceb
  working_tree: modified
  sha256: 7959f3aba83f147c150d6f2e933a53b02c8e900894b64f23be5fdbb8cdd6b097
---

# Python and Rust test structure

Follow the [development guide](../development.md#test-structure): prepare inputs,
execute the subject, then verify the result and related side effects. Blank lines
and descriptive names distinguish these phases without phase comments.[^development]

Split independent cases into separate tests or parameterized inputs. Keep retries,
concurrency and persistence sequences together when the sequence is the subject.
Exception assertions and fixture precondition checks need their natural placement.
Keep cleanup and property-test input generation intact.[^development]

Replace scenario-dependent branches with explicit inputs and expected outcomes,
or separate tests. Keep each assertion unconditional for its case. Platform/tool
eligibility belongs in skip fixtures or `cfg`; fixture builders and bounded test
drivers may retain construction/control logic.[^development]

Use property-based tests for wider domains with one invariant. Preserve explicit
boundary examples and shrinking. Keep finite categories and oracle rows exhaustive,
and avoid multiplying expensive subprocess cases.[^development]

Recheck this procedure when adopting a test framework or changing fixture ownership.
AAA is a readability convention; it does not establish coverage or correctness.
Run the relevant strict checks and tests after restructuring.[^development]

[^development]: [Development guide](../development.md#test-structure).
