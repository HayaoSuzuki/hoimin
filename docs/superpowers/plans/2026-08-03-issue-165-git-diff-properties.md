# Issue #165 hostile Git diff properties

Parent: [#165](https://github.com/tokyogas-tech/hoimin/issues/165)

The Git parser properties remain in `target::git`'s private `#[cfg(test)]`
module because they exercise private `parse_diff` and `decode_git_quoted`
directly. No production test hook or public test-support API is introduced.

## Acceptance traceability

| Criterion | Evidence |
|---|---|
| Generated zero-context diff sections preserve the generated destination ranges | `target::git::tests::generated_hostile_zero_context_diffs_match_ranges` |
| Source content beginning `++ b/evil.py`, `-- a/evil.py`, `@@ -1 +1 @@`, or `Binary files a/x.py and b/y.py differ` never becomes patch metadata | Every generated hunk contains all four hostile source lines; at least two hunks per section ensure a false path/header transition changes the observable range map |
| The expected range oracle is independent of production parsing | `expected_ranges` derives `LineRange` values only from generated hunk fields; `render_unified0_diff` only serializes the same test data model. Neither calls parser classifiers, path decoding, nor `parse_diff` |
| Every representable generated UTF-8 path survives Git C quoting and decoding | `target::git::tests::representable_utf8_git_c_quote_round_trips` |
| The encoder covers spaces, tabs, quotes, backslashes, and non-ASCII UTF-8 independently | `arbitrary_representable_utf8_git_path` includes all five classes and `git_c_quote` encodes bytes without calling or sharing escape parsing with `decode_git_quoted` |
| Arbitrary quoted decoder input remains total | Existing `target::git::tests::quoted_path_decoding_is_total` is retained rather than duplicated |

## TDD evidence

Two temporary production mutations proved the new properties detect their
targeted regressions before the final green run:

1. Removing the `AwaitingNewHeader` guard made a body line named
   `+++ b/evil.py` redirect the next generated hunk to `evil.py`; the hostile
   diff property failed with the independently expected destination map.
2. Decoding `\t` as a space made the C-quote round-trip property fail on its
   minimal mandatory tab-bearing path.

Both mutations were reverted. The focused final command is:

```console
PROPTEST_CASES=256 cargo test -p hoimin-cli target::git
```

