# Issue #160 Serialized Golden Artifacts

## Goal

Keep executable compatibility evidence for every serialized format that users may retain between
hoimin releases: complete schema-v2 JSON reports, schema-v2 JSONL event streams, and SQLite session
databases from schema versions 1, 2, and 3.

## Corpus

| Artifact | Compatibility contract |
| --- | --- |
| `golden/reports/schema-v2-original.json` | The initially published schema-v2 normalized-config shape, moved without a duplicate from `tests/fixtures`, with every optional available in that shape populated. |
| `golden/reports/schema-v2-current.json` | The current schema-v2 typed report shape with all optionals populated. |
| `golden/events/schema-v2-original.jsonl` | A complete typed lifecycle without the later verification-selection fields. |
| `golden/events/schema-v2-current.jsonl` | A complete current typed lifecycle with all optionals populated. |
| `golden/sessions/schema-v1.sqlite3` | A native v1 database with symbol, output metadata, and diagnostic values populated. |
| `golden/sessions/schema-v2.sqlite3` | The v2 schema and diagnostic index with the same populated logical rows. |
| `golden/sessions/schema-v3.sqlite3` | The current v3 schema with the same rows plus an `exit` termination and exit code. |

The report fixtures pin a normalized configuration, including `selection.diff_base`, output metrics,
and a session path. They also pin candidate symbol, mutant termination and output metadata, and a
non-null score. Current fixtures additionally pin verification-selection metadata on both
`run_started` and `run_finished`.

## Era boundaries

The initially published schema-v2 JSON shape predates `run.verification_selection` and
`summary.verification_selection`; those fields cannot be represented in that artifact and remain
absent. Its normalized configuration also intentionally remains at the initially published shape,
rather than acquiring later required non-optional configuration fields.

SQLite v1 and v2 predate `results.termination_kind` and `results.termination_exit_code`. Their
goldens therefore cannot encode a process termination. Migration to v3 must preserve those values
as null. SQLite v3 represents both fields and pins `termination_kind='exit'` with a non-null exit
code.

## Verification strategy

- Both JSON reports are consumed through `read_report`; the original is also checked against the
  published JSON schema.
- Every JSONL line is deserialized as `OutputEvent`, and every full stream is accepted by
  `ReportSequence` before its optional fields are asserted.
- Each SQLite artifact is inspected directly before migration for its exact `user_version` and
  era-representable rows. A copied artifact is then opened through `SessionHandler`, looked up as a
  typed stored result, and inspected after migration to v3 for semantic preservation.
- Current JSON and JSONL are regenerated from fixed typed events and compared as typed values.
- Current SQLite is regenerated through `SessionHandler`; comparison covers `sqlite_master` SQL,
  `user_version`, and ordered logical rows. Database bytes, page counters, timestamps, and generated
  row IDs are deliberately excluded.

All constructors and assertions stay in the consuming integration suites. No public test-only API
or shared fixture crate is introduced.
