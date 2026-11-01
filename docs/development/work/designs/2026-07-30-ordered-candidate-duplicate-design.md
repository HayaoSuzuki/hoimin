# Ordered Candidate Duplicate Handling

## Context

`RunState::with_ordered_candidate_filter` accepts a public `Vec<String>`. The
state currently stores that vector unchanged while using a set for discovery.
At end-of-spool, duplicate IDs pass presence validation but cause the same
candidate to be removed from the discovery map twice, so the second removal
panics.

## Decision

The constructor stable-deduplicates candidate IDs. The first occurrence of
each ID is retained and later occurrences are ignored. Execution therefore
remains deterministic and preserves the caller's requested order for unique
IDs.

This policy is documented on the constructor. It preserves the existing
infallible public API, unlike changing the constructor to return a typed
error. Deduplicating at construction also keeps the membership filter and
ordered state consistent throughout the machine lifetime.

## Data Flow

The constructor traverses the input once. An ID is inserted into the
membership set and appended to the ordered vector only when it was not already
present. Candidate discovery and end-of-spool collection then operate on the
same unique ID sequence.

Missing unique IDs continue to return `MachineError::SelectedCandidateMissing`.
No new error type or CLI behavior is introduced.

## Testing

A machine regression test supplies duplicate requested IDs, drives candidate
loading through end-of-spool, and asserts that:

- the end-of-spool transition does not panic or return an error;
- only one execution is scheduled for each unique ID; and
- the first-occurrence order is preserved.

The existing ordered-selection and cancellation tests continue to protect
normal ordering behavior.

## Documentation Scope

The duplicate policy is a core API contract and belongs in the constructor's
Rust documentation. The README does not describe this low-level constructor,
so it does not require an update.
