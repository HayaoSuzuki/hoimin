# Ruff 0.17.2 source snapshot for hoimin

The parser is copied from [Ruff 0.17.2](https://github.com/astral-sh/ruff/tree/30516b42ab972ffa2569e5efcfee81135da466d3), commit `30516b42ab972ffa2569e5efcfee81135da466d3`. Its internal crate version is `0.0.19`. Only the parser is vendored because it retains the safety adaptations below. Its aligned AST, trivia, text-size and Unicode dependencies come from crates.io version `0.0.19`; direct Ruff dependencies use exact version requirements. Both the main and fuzz lockfiles record registry checksums. The published packages' VCS metadata identifies the same upstream commit.

The parser manifest expands upstream workspace inheritance, omits upstream test/benchmark dependencies, and explicitly lists build input patterns to exclude temporary files from sdists. It is excluded from hoimin workspace membership and has its own workspace boundary. Hoimin's integration tests exercise parsing and recovery; upstream's test harness is not packaged as a standalone suite.

Direct git dependencies were initially tried but Cargo's checkout failed on Windows at a long Ruff linter snapshot filename. Published registry dependencies avoid that checkout entirely. Source distributions retain the patched parser; rebuilding also downloads normal crates.io dependencies under the committed Cargo.lock.

## Local parser changes

- Enable the upstream `stacker` feature by default. Preserve a 128 KiB red zone, 1 MiB growth segments, and a stack probe at every recursion checkpoint (zero unchecked recursion entries).
- Guard helper traversals for expression context, pre-3.9 decorators and pattern conversion, plus assignment/delete target validation.
- Preserve iterative `ast_cleanup` for speculative with/match trees, invalid keyword patterns and discarded `as` subpatterns. Hoimin retains post-parse depth-128 rejection and iterative destruction of rejected/partial trees.
- Accept either f-string or t-string middle token during interpolation recovery, retaining the mixed-string crash fix.

These protections are checked by the `analysis_depth` integration suite and `hoimin-python` depth/reclamation tests. Dynamic stack growth is subject to platform support and allocation availability; it is not a total memory bound.

## Source and license verification

The external Ruff packages are unmodified registry releases from the pinned upstream commit. Parser source changes are limited to the safety adaptations above. The original upstream fixtures under `resources/` predate this refresh and are not a supported test harness; hoimin owns the executable regression suite.

The parser retains the upstream MIT license. External Unicode packages retain their original MIT/Apache and Unicode notices. `ruff_python_parser/LICENSE` combines Ruff's full license/third-party notices with the Unicode notices so the wheel and standalone archive license path also carries the relevant dependency notices.

CI runs workspace formatting/Clippy, standalone parser formatting, and `cargo clippy --locked -p ruff_python_parser --lib --no-deps -- -D warnings`. Release SBOMs identify the vendored parser with this upstream revision and the hoimin source commit, with a pinned reference and digest for this document. External Ruff components retain their captured registry package identities and metadata.
