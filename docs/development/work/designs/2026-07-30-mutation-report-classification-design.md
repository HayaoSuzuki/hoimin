# Mutation Report Classification Design

## Scope

Fix #65 so an `unviable` mutation result remains visibly unverified and is
recommended for follow-up, matching the documented focused-mutation policy.

## Design

Keep the report's existing policy that `killed` and `survived` are conclusive
execution outcomes. Define verified candidates as exactly those two states.
All other states remain unverified. The investigation section continues to
contain survived, timeout, unviable, and error outcomes.

Add an explicit unverified count to the report header so readers do not need to
infer completeness from section contents. Recommendation order remains record
order across all unverified candidates.

## Testing

Build one report containing every `CandidateState` and assert section
membership for verified, investigation, unverified, and recommendation
sections. Specifically assert that `unviable` is absent from verified, present
in investigation and unverified, and appears in the recommendation order.
Assert the explicit unverified total.
