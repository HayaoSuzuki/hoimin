# Focused Mutation Git Encoding Design

## Problem

The focused-mutation probe and startup repository lookup ask `subprocess.run` for locale-decoded text. On Japanese Windows this selects cp932, while Git for Windows emits unquoted `-z` paths and repository names as UTF-8. A non-ASCII path can therefore abort discovery with `UnicodeDecodeError` or become unusable mojibake.

## Decision

Pin both direct Git subprocess call sites to `encoding="utf-8"` and `errors="surrogateescape"`. UTF-8 matches Git for Windows. `surrogateescape` rejects neither legacy POSIX filename bytes nor silently replaces them, allowing Python filesystem APIs to round-trip raw bytes on platforms that support such names.

Keep `text=True` so the existing `CommandProbe` contract remains string-based. Do not change command output persisted by `CommandRunner`, which already uses explicit UTF-8 handling.

## Verification

Mock-based regressions require both subprocess call sites to pass the explicit encoding and error policy and exercise a Japanese repository path. Existing focused-mutation discovery/reporting tests and targeted mutation tests must remain green.
