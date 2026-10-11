# Fuzzing Adoption Assessment Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline and request one fresh independent review at the end.

**Goal:** Produce reproducible adoption decisions for #805 and #807.

**Architecture:** Inspect current properties and bounded CI evidence first.
Keep experimental adapters outside production crates and publish limitations
alongside results. Adopt no framework without a demonstrated maintenance benefit.

**Tech Stack:** Python 3.14, Rust 1.99, existing proptest, optional Bolero probe.

**Spec:** [Bounded assessment](../specs/2026-10-12-fuzz-adoption-design.md).

## Global constraints

One worker, existing resource guards unchanged, no HypoFuzz adoption, no #806
implementation, no new product input surface. Rust probe setup/build <=10 minutes,
each execution <=60 seconds; clean its dedicated build directory afterwards.

## Review focus

- Compressed caches do not measure initialization time.
- Rounded libFuzzer execution rates only permit an approximate replay estimate.
- Shared oracles can share mistakes; independent sampling regressions stay.
- Random-engine results do not establish libFuzzer feature preservation.
- Missing Linux tools must result in an explicit limitation, not invented data.

## Tasks

- [x] Extract source identity, stage times and initialization observations from
  run 38172184143; retain small evidence and acquisition instructions in the report.
- [x] Exercise a dependency-free sampling oracle on fixed boundaries; deliberately
  corrupt results to verify sensitivity before randomized adapter runs.
- [x] Compare proptest/Bolero adapters only within the setup budget; record
  setup, execution, replay and source-maintenance costs separately.
- [x] Assess #804's initial target and #807's candidate list; record adoption,
  rejection or a concrete bounded follow-up for each.
- [ ] Verify research material, update knowledge/source catalogs, obtain an
  independent review, commit and open reviewable PR(s) through `gh stack`.

## Plan self-review, five separate passes

1. Preconditions: checked clean research branch based on current main and the
   initialized stack; no edits on main.
2. Costs: selected existing small artifacts instead of downloading all caches
   or rebuilding all eight fuzz targets.
3. Probe validity: planned corruption checks and fixed-input replay before any
   randomized comparison; no performance conclusion from different domains.
4. Stop conditions: missing platform support and setup timeout are recorded
   outcomes; speculative CI/framework expansion is excluded.
5. Delivery: planned both source catalogs, a knowledge decision, recorded
   reviews, PR attachment and issue-specific outcome comments.


## Execution ruling

Independent review found the suspected #807 compatibility gap was already
covered by the committed Lean corpus. Removed the redundant trial tests and
kept the negative assessment. Consolidate the local two-branch draft into one
research PR; there is no independent implementation change to stack.
