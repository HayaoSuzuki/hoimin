# Evidence archive

The Task1–9 reports and reviews are byte-preserving copies of the authoritative
ignored SDD originals. Their original task status, initial failures and controller
measurement corrections remain intact. `execution-ledger.md` captures all rulings
and historical progress at Task10 start. The controller will append Task10 and
subsequent review evidence after those records exist.

`probes/` contains bounded reproduction drivers; `observations/` contains raw
observations and the pinned input inventory. No upstream source files, binaries
or environment trees are archived. Fetch pinned inputs into an owned temporary
working directory: the existing `docs/performance/probes/fetch-analysis-inputs.py`
writes a historical relative inventory path, so do not run it from the repository
just to prepare these fixtures. Compare input bytes and SHA-256 with the inventory
before reproducing the probes. Commands and binary identities are in the reports
and observations; script defaults alone do not identify the recorded experiment.

`manifest.json` preserves the original early ten-report staging inventory. It is
historical, not the complete archive inventory. `SHA256SUMS` covers the current
archive recursively, excluding itself. Refresh it when appending later evidence.
The root [maintenance summary](../2026-10-18-issues-971-979.md) provides the current
entry point and limits. [Acceptance reconciliation](acceptance.md) maps the
approved issue lists to their observed evidence.

The maintained `probes/check-okf.py` runs from the repository root with PyYAML.
It checks structural rules, source footnotes, local targets, reachability and
new-source hashes/revision blobs, then compares catalog findings with the exact
recorded baseline. It does not hide those pre-existing findings or claim complete
catalog coverage; `okf-audit.json` retains their individual identities. Content
review remains separate from this mechanical check.

`task-10-fix1.json` audits all manually assembled check rows and explains the
correction to the initial checker-success receipt. Current `integration-gates.json`
uses the corrected receipts; `observations/task-10-integration-gates-original.json`
preserves the original bytes as historical evidence. The original checker logs are preserved in
`observations/`; they already failed the tuple/list baseline comparison. The fix
normalizes only representation and records the successful 18-source check.
