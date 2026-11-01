# Issue 463: JSON mutant record writes

Issue: https://github.com/tokyogas-tech/hoimin/issues/463

## Decision

`JsonReport::write_mutant` serializes the optional separator and one complete `MutantFinished` event into a local `Vec<u8>`, then sends that record to the spool with one `write_all`. The buffer lasts for one event, so retained memory remains independent of mutant count and disk remains the report history store.

The handler marks the report poisoned before serialization and restores `Open` only after `write_all` succeeds. `write_all` handles legal short writes, while `WriteZero` and later I/O errors fail the original effect before `OutputEmitted` is returned. `has_mutants` changes only after the complete record succeeds.

A persistent `BufWriter` was rejected because it can acknowledge an event while bytes and an eventual I/O error remain in memory. Flushing it after every event would recover the acknowledgment contract but complicate the existing `Read + Write + Seek` final-report path without improving the record-local approach.

## Verification

A counting spool checks one underlying write call per mutant and byte-for-byte report output. A bounded short writer checks `write_all`; partial-failure fixtures check immediate typed failure and poisoned retry. The existing report heap test checks count-independent peak memory. Release measurement compares the actual handler path at a representative event count.

## Design self-review

1. Acknowledgment: no success leaves bytes buffered; the spool write finishes within `record`.
2. Failure: short writes retry, errors and zero progress fail before acknowledgment, and every failed attempt remains poisoned.
3. Resources: allocation is bounded by one event rather than event count; the existing managed delivery spool and final flush/seek/copy lifecycle are unchanged.
