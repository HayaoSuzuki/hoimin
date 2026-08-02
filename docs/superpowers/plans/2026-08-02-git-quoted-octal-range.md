# Git Quoted Octal Range Plan

1. Add regression tests for `\\400` and `\\777`, plus an arbitrary-input no-panic property.
2. Confirm the boundary regressions fail against the current `u8` fold.
3. Fold octal digits into `u16` and reject failed `u8` conversions with the existing octal error.
4. Run focused tests, formatting, clippy, the full workspace suite, and diff validation.
5. Request independent review, create a PR, wait for all CI jobs, and merge after success.
