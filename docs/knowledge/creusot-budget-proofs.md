---
type: Contract
title: Production budget arithmetic proofs
description: Shared production bodies, full typed input domains, fresh three-target proofs and their limits.
status: draft
sources:
- id: guide
  resource: ../../formal/creusot/README.md
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: modified
  sha256: 0d35745e0754906b666f30c83f9d24bf1a8a4d7b612c86eb5259d20ef9e29b7a
- id: manifest
  resource: ../../formal/creusot/obligations.json
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: clean
  sha256: 529afb6307b2d09eccef2267a471a4d9cd41ee491aa632720ce5abc49b0e95a3
- id: runner
  resource: ../../tools/verify_creusot.py
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: clean
  sha256: 57e65a4b70eb7bd7f6fb171710da0146bcbe10844800861bf46ca45b55f12535
- id: record
  resource: ../superpowers/reports/2026-10-19-issue-999-creusot-adoption.md
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: untracked
  sha256: 5c69ecd723233a0b9068e438ace77132d983606fdea360da4fc3c297b51c8a86
---

# Production implementation and contract

`hoimin-budget-math` owns duration value types, automatic timeout and top-budget
projection. Core and CLI use those same types and function bodies. Conditional
Creusot imports, models and attributes add proof obligations without replacing
the runtime arithmetic. Stable Rust 1.99.0 and ordinary all-features builds need
no proof tools.[^guide]

The contracts cover mathematical ceiling for waves, both auto-timeout saturation
steps and its floor, Fixed/Auto selection, duration-product saturation including
u128 overflow, exact nanosecond reconstruction, strict shortfall, input
preservation and positive-duration invariants. They quantify over all public
`usize`, positive jobs, `Duration` and positive fixed-duration inputs. The public
projection accepts manually constructed field combinations.[^manifest]

# Proof execution and trust

The required Linux job generates and proves `x86_64-unknown-linux-gnu`,
`aarch64-apple-darwin` and `x86_64-pc-windows-msvc` separately and sequentially.
Each run verifies pinned tools, effective project configuration, source contracts,
nonempty fresh Coma and final named goal results. Saved strategies guide new
solver runs; `--no-cache --replay` does not reuse successful answers. Final
unproved goals, missing tools, timeouts and unintended skips fail CI.[^runner]

The trust boundary includes official Rust/Creusot standard-library specifications,
compiler, Why3/Why3find and Z3/CVC5. Hoimin adds no trusted code, external
specification or assumption to close a goal. Each solver has a 30-second wall
limit and 2-GiB address-space cap; two solvers may run concurrently. The complete
CI job, including setup, has 30 minutes. Address space and observed process RSS
are distinct from an aggregate environment memory limit.[^guide]

# Evidence and maintenance

Native Linux x86_64 source `41c888f5`, run `39191762043`, passed both a 240-second
warm job and a 942-second genuine cold job, with 25 generated files and 59 goals
per target. Five compiling arithmetic/equality mutations failed the corresponding
goals. The adoption record preserves exact revisions, failures, repair costs,
local ARM measurements, later integration results and review dispositions.[^record]

Follow the [proof guide](../../formal/creusot/README.md) for regeneration, replay,
strategy repair and pinned upgrades. Recheck source inventories, sensitivity,
normal builds, licenses, SBOMs, sdist and wheel inputs after relevant changes.
Absolute source spans resolve against the live proof checkout; regenerate for a
new path. Archived warnings from older relative-span runs remain historical
evidence.[^guide]

# Exclusions

These proofs do not establish BudgetLedger transitions, reservation ownership,
I/O, persistence, scheduling, OS timing, serialization or unsupported targets.
The projection remains warning-only. Lean model proofs, implementation corpus
correspondence, runtime contracts and native process tests retain their separate
roles; a proof of the arithmetic does not replace those checks.[^guide]

[^guide]: [Proof and maintenance guide](../../formal/creusot/README.md).
[^manifest]: [Required contracts and goals](../../formal/creusot/obligations.json).
[^runner]: [Fresh proof runner](../../tools/verify_creusot.py).
[^record]: [Adoption evidence and review record](../superpowers/reports/2026-10-19-issue-999-creusot-adoption.md).
