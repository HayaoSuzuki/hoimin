# Issue #328: augmented assignment and match boolean operator gaps

## Scope and cause

This change fixes issue #328 on branch `fix/issue-328-operator-gaps`, based on
`72d5f7c`. The Rust analyzer's raw-token pass requires each token start to come
from an AST role allowlist. `Stmt::AugAssign` admitted only `+=` and `-=`, and
the replacement catalog had no entries for the other requested augmented
arithmetic pairs. The analyzer therefore omitted `*=`/`/=` and `//=`/`%=`.

The AST represents expression booleans as `Expr::BooleanLiteral` and represents
`True` and `False` in match patterns as `Pattern::MatchSingleton`. `AstFacts`
visited the expression form but did not record singleton-pattern token starts,
so the existing `boolean_literal` replacement never reached those patterns.

## Implementation and compatibility

The operator catalog now exposes `augmented_mul_div` and
`augmented_floor_mod`. Both operators belong to the default runtime selection
and the arithmetic ranking category. The token pass maps `*=` to `/=` in both
directions and maps `//=` to `%=` in both directions. The AST allowlist admits
those spellings only in the gap between an augmented assignment's target and
value.

`AstFacts::visit_pattern` records `True` and `False` for boolean
`MatchSingleton` nodes and lets the standard pattern walker find nested
singletons. The value check excludes `None`; the node-kind boundary excludes
wildcard, capture, and string-value patterns.

The default runtime operator count increases from 31 to 33. Runs that use the
default selection can discover more candidates, and the normalized operator
set changes their configuration fingerprint. Existing plan and session
compatibility checks therefore keep results produced under the old default
separate from results produced under this default. Persisted configurations
that name an explicit operator set retain their recorded selection.

## TDD and verification evidence

The first analyzer test run failed because all four multiplicative augmented
assignment candidates were absent. The pattern regression produced no
`boolean_literal` candidates, the focused line-and-symbol selection regression
produced no candidates, and the core catalog regression could not find the new
default IDs. After the production changes, these tests passed. The pattern
fixture covers direct and nested sequence, mapping, and OR patterns; it also
checks line, column, span length, symbol, parseability, and the absence of
candidate output for `None`, wildcard, capture, and string patterns.

Final checks used `CARGO_TARGET_DIR=/private/tmp/hoimin-issue-360-target`:

| Command | Result |
| --- | --- |
| `cargo test -p hoimin-cli --lib -- --test-threads=1` | 537 passed; 9 ignored |
| `cargo test -p hoimin-cli --test cli_config -- --test-threads=1` | 55 passed |
| `cargo test -p hoimin-core -- --test-threads=1` | Exit 0 |
| `cargo test --workspace --all-features -- --test-threads=1` | Exit 0; all workspace, integration, doc, and Lean contract tests passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0 |
| `cargo fmt --all -- --check` | Exit 0 |
| `git diff --check` | Exit 0 |

The macOS workspace test used Python 3.14.7 through a temporary `.venv` link to
the repository's development environment. The link was removed after the test.
No formal corpus file changed.
