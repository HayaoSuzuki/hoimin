---
type: Playbook
title: AAA test structure
description: Arrange fixtures, execute the subject and assert related outcomes in Python and Rust.
status: draft
catalog_revision: a7d94ab3d08877d9f615122ca8b5deccb1042ceb
sources:
- id: development
  resource: ../guides/contributing.md
  revision: 3437cf034816b1aff2372865e0966413442ca2ff
  working_tree: modified
  sha256: 5742f1a39fb5201ff317b067096cc42f5652060a3db8b139563f92f8aae5a2d5
---

# Python and Rust test structure

Follow the [development guide](../guides/contributing.md#test-structure): prepare inputs,
execute the subject, then verify the result and related side effects. Blank lines
and descriptive names distinguish these phases without phase comments.[^development]

Require one case per invocation, including each parameterized or generated input.
Keep related result and side-effect assertions together. Split independent cases
into separate tests or parameterized inputs. Keep retries, concurrency and
persistence sequences together when the sequence is the subject.
Exception assertions and fixture precondition checks need their natural placement.
Keep cleanup and property-test input generation intact.[^development]

Replace scenario-dependent branches with explicit inputs and expected outcomes,
or separate tests. Keep each assertion unconditional for its case. Platform/tool
eligibility belongs in skip fixtures or `cfg`; fixture builders and bounded test
drivers may retain construction/control logic.[^development]

Parameterize repeated test logic with `pytest.mark.parametrize` in Python and
named `rstest::rstest` cases in Rust. Give cases descriptive IDs and prepare fresh
state for each invocation. Replace loops over independent scenarios with separate
cases.[^development]

Review parameter tables when changing tests and convert sampled wider domains
with one invariant to Hypothesis or proptest. Generate values beyond the original
table. Preserve explicit boundary/regression examples and shrinking. Keep finite
categories and oracle rows exhaustive, and avoid multiplying expensive subprocess
cases.[^development]

Recheck this procedure when adopting a test framework or changing fixture ownership.
AAA is a readability convention; it does not establish coverage or correctness.
Run the relevant strict checks and tests after restructuring.[^development]

[^development]: [Development guide](../guides/contributing.md#test-structure).
