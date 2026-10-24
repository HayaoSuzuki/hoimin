# Ruff 0.17.2 source snapshot for hoimin

The parser and its eight Ruff dependencies are copied from [Ruff 0.17.2](https://github.com/astral-sh/ruff/tree/30516b42ab972ffa2569e5efcfee81135da466d3), commit `30516b42ab972ffa2569e5efcfee81135da466d3`. Their internal crate version is `0.0.19`. The source closure comprises `ruff_python_parser`, `ruff_python_ast`, `ruff_python_trivia`, `ruff_source_file`, `ruff_text_size`, `ruff_unicode_names2`, `ruff_unicode_names2_generator`, `ruff_cache`, and `ruff_macros`. Optional cache/macro sources are included so Cargo can resolve the manifests without the Ruff workspace.

The source snapshot includes upstream `src/` and the Unicode build script/data. Manifests expand upstream workspace package/dependency inheritance, replace Ruff paths with sibling vendor paths, omit upstream test/benchmark dependencies, and explicitly list build input patterns to exclude temporary files from sdists. Each package is excluded from hoimin workspace membership and has its own workspace boundary. Hoimin's integration tests exercise parsing and recovery; upstream's test harness is not packaged as a standalone suite.

Direct git dependencies were tried but Cargo's checkout failed on Windows at a long Ruff linter snapshot filename. Vendoring the required crates avoids making users configure Windows long-path support and makes sdists self-contained apart from normal crates.io dependencies.

## Local parser changes

- Enable the upstream `stacker` feature by default. Preserve a 128 KiB red zone, 1 MiB growth segments, and a stack probe at every recursion checkpoint (zero unchecked recursion entries).
- Guard helper traversals for expression context, pre-3.9 decorators and pattern conversion, plus assignment/delete target validation.
- Preserve iterative `ast_cleanup` for speculative with/match trees, invalid keyword patterns and discarded `as` subpatterns. Hoimin retains post-parse depth-128 rejection and iterative destruction of rejected/partial trees.
- Accept either f-string or t-string middle token during interpolation recovery, retaining the mixed-string crash fix.

These protections are checked by the `analysis_depth` integration suite and `hoimin-python` depth/reclamation tests. Dynamic stack growth is subject to platform support and allocation availability; it is not a total memory bound.

## Source and license verification

All non-parser Rust sources and Unicode data are unchanged from the pinned upstream commit. Parser source changes are limited to the safety adaptations above. The original upstream fixtures under `resources/` predate this refresh and are not a supported test harness; hoimin owns the executable regression suite.

Each Ruff crate retains the upstream MIT license. Unicode sources also retain their original MIT/Apache and Unicode notices. `ruff_python_parser/LICENSE` combines Ruff's full license/third-party notices with the Unicode notices so the existing wheel and standalone archive license path carries every vendored notice. Individual Unicode notices remain in the source distribution as well.

CI runs workspace formatting/Clippy, standalone parser formatting, and `cargo clippy --locked -p ruff_python_parser --lib --no-deps -- -D warnings`. Release SBOMs identify every included vendored Ruff crate with this upstream revision and the hoimin source commit, with a pinned reference and digest for this document.
