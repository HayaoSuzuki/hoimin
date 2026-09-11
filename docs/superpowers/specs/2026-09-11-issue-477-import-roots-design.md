# Issue 477: Configure worker import roots independently of mutation selection

Issue: https://github.com/tokyogas-tech/hoimin/issues/477

## Cause and chosen contract

`--file src/pkg/calc.py` selects the correct mutation file, but environment construction only adds the worker root and `selection.sources` to PYTHONPATH. A regular package installed through a path-only editable `.pth` can therefore import original-root/src/pkg/calc.py during both baseline and mutant execution. Adding `--source src` changes selection as well as imports, so it cannot represent a one-file request.

Add repeatable `--import-root DIR` to run and plan, persisted as ordered `import_roots` separate from `Selection`. Each directory is project-root-relative, including `.`; reject absolute paths and escaping parent components rather than interpreting them as external dependencies. Normalize harmless `.` components and duplicate roots while preserving first occurrence/order. The setting does not satisfy the required mutation-selector condition and never adds targets. Validate normalized configuration too, including saved plans, so deserialization cannot bypass the path contract.

Environment precedence is worker root, explicit worker import roots in user order, existing source roots in existing order, then rewritten inherited PYTHONPATH. Use existing OS-aware split/join and deduplication; never edit the user's `.pth`, venv, original source, or inject a Python import loader. `--file` and `--line` retain their original selection scopes. Explicit import roots do not bypass the existing workspace copy policy; require the corresponding worker directory to exist before baseline to avoid silently falling back through `.pth` when the named root is absent or excluded. Report a useful execution diagnostic for unavailable copied roots. Check copied directories in the existing owned blocking CreateWorker stage before WorkerCreated, and in the synchronous handler equivalent. Keep rejected workspaces under pending_cleanup ownership. Environment assembly remains free of filesystem reads; core validation remains pure.

## Configuration and compatibility

Thread the dedicated field through RawRunConfig, RunConfig, PlanConfig and CLI conversions to WorkspaceHandler, keeping selection and import concerns distinct. Use a small builder or clear dedicated parameter; do not pass import roots as fake selected sources. Preserve absent-field decoding as empty where historical run reports need it, and inspect JSON Schema's extension contract before changing it.

Advance actual plan schema 3 to 4 because the saved execution settings gain import semantics, and reject old schemas before baseline with regeneration guidance. Ranking rule stays 3 on this independent base branch; issue473 changes ranking separately. Advance fingerprint schema 6 to 7 and frame the ordered normalized import-root list under a new field tag. Different import-root precedence can change a verdict and must not reuse the same session fingerprint. Old session fingerprints follow existing outdated-schema diagnostics; do not rewrite stored results. Empty roots preserve existing execution behavior but use the new fingerprint version. Document regeneration/new-session costs.

## Scope and alternatives

Explicit import roots cover path-only `.pth` regular packages when Python honors PYTHONPATH. Existing `PYTHONPATH=/project/src` rewriting remains a documented workaround. Finder-based editable installations, custom sys.meta_path loaders and Python `-E`/`-I` flags are not guaranteed by this setting. Auto-inspecting arbitrary `.pth` contents or inferring package roots from one selected file would add ambiguity and execute or reinterpret environment-specific behavior; those alternatives are outside this bounded change.

Python primary references (consulted 2026-09-11): [site path configuration](https://docs.python.org/3/library/site.html), [PYTHONPATH and ignored-environment flags](https://docs.python.org/3/using/cmdline.html#envvar-PYTHONPATH). Their contracts explain why prepended worker directories cover path entries but cannot promise every custom finder.

## Verification

Capture semantic RED with a network-free venv `--without-pip`, regular pkg/__init__.py and path-only `.pth` pointing to original src. An external log records actual imported `__file__` and add(2,3): old --file path imports original and records5/5, while the fixed explicit-root run records baseline5 and mutant-1 inside worker and reports killed. Keep an unrelated Python file with eligible mutations and assert it never becomes a candidate. Repeat with line selection. Use actual plan serialization followed by actual CLI verify (without repeating --import-root) and inspect persisted import roots, selected candidate, imported paths, unchanged source/plan bytes.

Unit/integration boundaries: CLI repeat/order/path errors; import-root alone cannot select targets; normalized plan tampering rejected; ordering and deduplication with existing source roots/PYTHONPATH; missing/excluded root rejected before command marker; fingerprint different values/order, unchanged equivalent normalization, old schema diagnostics; default roots preserve existing environment behavior. Exercise current report/schema and session regressions. Lean is not needed to prove Python import behavior; existing fingerprint/session correspondence tests remain useful but do not constitute a proof about Python loaders.

Run focused regressions, full workspace all-features, fmt and Clippy. Every stage records three concrete self-reviews, and task/final reviewers inspect the final evidence. IDE MCP is unavailable for this checkout because hoimin is not open; Cargo diagnostics provide Rust verification.

## Design self-review

1. Followed CLI→core→plan→shell→workspace environment construction: selection currently doubles as the import-root input. Dedicated ordered roots solve the issue without expanding targets or altering mutation delivery.
2. Checked compatibility: actual plan schema3 and fingerprint6 require explicit migration policy; order affects Python module precedence, so fingerprint encoding must preserve it. Default historical report decoding and extension-friendly schemas need separate treatment from plan version rejection.
3. Checked observable safety: public `.pth` fixture records imported paths and values for both phases, unavailable worker roots cannot silently trigger original-source fallback, and documented support excludes custom finders/ignored environment flags. Root copy checks remain in workspace execution, not pure config.
