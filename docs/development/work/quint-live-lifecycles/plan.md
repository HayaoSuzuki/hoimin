# Live lifecycle implementation plan

**Goal:** Add bounded disk-observation modeling and live BudgetLedger correspondence.
**Architecture:** New finite disk model; typed wrappers reuse existing budget transitions; isolated Connect drivers compare public observations.
**Tech Stack:** Pinned Quint 0.33.1, TLC, quint-connect 0.1.2, rstest, proptest.
**Spec:** [Design](design.md)

## Constraints

Keep existing 20s/2048MiB model and 300s/2048MiB Connect guards. Build with two Cargo jobs and serial live tests. Preserve ordinary offline tests, fixed traces, seeds and fuzz targets. One invocation per case, AAA; loops belong to drivers. Commit each item separately. Clean only after builds stop.

## Review focus

- Both thresholds reached: reserve reason wins, size evidence remains secondary.
- Repeated evidence: stable order, no duplicates, original primary preserved.
- Different measurement duration: retain distinct evidence.
- Budget rejection: ID and existing reservations do not change.
- Missing implementation action: framework state comparison must fail semantically.

## Item 1: Disk observation

- [x] Add failing live cases, then implement `formal/quint/disk_observation.qnt` and `connect/src/disk_observation.rs`.
- [x] Register positive/negative TLC and fixed ITF; add ordinary replay in `crates/hoimin-core/tests/quint_disk_observation.rs` and shared support driver.
- [x] Check threshold/priority, payload, ordered dedup, dispatch gate; literal scenarios to seed PBT; preserve existing disk fuzz laws.
- [x] Run model/Connect/replay/lint checks, record five implementation/test review passes, commit.

## Item 2: Budget ledger

- [x] Add failing live cases; introduce typed live wrapper sharing the existing model transitions.
- [x] Implement state projection and real reserve/release calls; add focused schedules, seed PBT and semantic negative control.
- [x] Check existing fixed traces unchanged, full registry, both contract profiles and relevant tests.
- [x] Record five implementation/test review passes, commit; run cargo clean after builds stop.
