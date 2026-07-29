# Mutation Report Classification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make focused mutation reports classify every candidate state consistently with the documented verification policy.

**Architecture:** Keep classification local to the Markdown renderer. Reduce the conclusive state set to killed and survived, derive unverified candidates as its complement, and render an explicit count.

**Tech Stack:** Python 3.14, unittest

## Global Constraints

- `unviable` is never verified.
- `survived` remains a conclusive execution result and an investigation item.
- Recommendation order follows candidate record order.
- JSON state serialization is unchanged.

---

### Task 1: Specify every state's report membership

**Files:**
- Modify: `tests/test_focused_mutation_reporting.py`

**Interfaces:**
- Consumes: `render_markdown(RunRecord) -> str`
- Produces: a state-table regression test for all report sections

- [ ] **Step 1: Add the all-state report test**

Create one candidate per `CandidateState`, split the rendered Markdown into its
sections, and assert literal expected symbol sets and recommendation order.
Assert the unverified count is five.

- [ ] **Step 2: Verify RED**

Run: `./.venv/bin/python -m unittest tests.test_focused_mutation_reporting.FocusedMutationReportingTests.test_every_candidate_state_has_documented_report_membership -v`

Expected: FAIL because `unviable` appears as verified and the count is absent.

### Task 2: Correct renderer classification

**Files:**
- Modify: `tools/focused_mutation_support/reporting.py`
- Test: `tests/test_focused_mutation_reporting.py`

**Interfaces:**
- Consumes: `CandidateState`
- Produces: Markdown with consistent verified/unverified membership and count

- [ ] **Step 1: Remove `UNVIABLE` from conclusive states**

Leave killed and survived as the only verified states.

- [ ] **Step 2: Render the unverified total**

Add `- Unverified candidates: <count>` to the report summary header.

- [ ] **Step 3: Verify focused and full Python suites**

Run:

```console
./.venv/bin/python -m unittest tests.test_focused_mutation_reporting -v
./.venv/bin/python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: all tests pass, with only platform-gated Windows tests skipped on
non-Windows hosts.
