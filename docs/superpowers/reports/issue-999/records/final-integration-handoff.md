# Final integration pointers

- docs/development.md documents architecture and Lean/runtime commands (large current file; add a focused section/link, no broad rewrite).
- docs/knowledge/release-sbom.md explicitly says shipping graph contains exactly one hoimin-core and one hoimin-python; update for math dependency.
- docs/knowledge/dependency-policy.md, docs/dependency-policy.md and docs/dependency-build-scripts.md track dependency policy, narrow license exceptions and build-script inventory. Recheck creusot-std/proc macros after Task3; do not broaden global license allowlist.
- deny.toml has per-internal-crate Elastic exception; Task1 implementer notified to add math crate narrowly if needed.
- docs/knowledge/rust-toolchains.md should distinguish Creusot nightly from normal stable and randomized/fuzz nightly. No MSRV downgrade.
- docs/knowledge/ci-validation.md and docs/ci.md document required CI semantics and tools. Current transferred-repo branch-protection settings were not configured at their inspection; integrating mandatory job into CI result does not authorize editing remote branch protection.
- docs/knowledge/index.md and references/design-documents.md/audit-documents.md likely need new contract/report entries. Follow existing OKF workflow and create-okf skill for metadata/source fingerprints; do not rewrite all historical source hashes.
- Updated tool files capture_sbom.py/sbom.py/release.py and additional root/fuzz dependencies need concrete generated sdist/SBOM/wheel smoke acceptance, not just fixture tests.
