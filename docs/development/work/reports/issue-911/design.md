# Issue 911: typed verification selection

## Intent and boundary

The approved issue replaces independently writable selection discriminants with
one owner for their invariants. Keep the current CLI, sampling algorithm, dispatch
order and writer-produced JSON shape. Top retains three policies. Reports allow
selected=0 for non-sampled selections. Additive report metadata remains accepted.

`VerificationSelection` will hold private common counts and a private enum:
CandidateIds { plan_truncated }, Top { policy, plan_truncated }, or
Sample(VerificationSampling). Scope, fixed policies, mode and sampled truncation
are derived, not stored. Top uses a three-variant policy type. Constructors and
Deserialize share validation. Sampling owns a positive population and nonempty,
unique ordered IDs bounded by that population. Counts require requested>0 and
selected<=requested; sampled selected must equal the evidence length.

Keep mode/policy/scope vocabulary enums as read-only projections. They are not
compatibility constructors. A wire-only input struct validates known fields,
including present-null sampling, while ignoring additive selection fields.
Serialization writes the existing flat shape without cloning the ID list.

The three existing CLI selection branches construct validated metadata directly.
Human output and preview use typed projections; they do not serialize to JSON
and parse names back out. RunState inspects the derived mode for ordered verify.
No changes to the worker or ordered-candidate lifecycle belong here (#912).

## Compatibility decision

Normal emitted JSON, schema numbers and algorithms remain unchanged. The input
boundary becomes stricter for contradictory known fields, zero requested counts
and invalid sampling evidence. Align the two current JSON Schemas with the
existing meaning of each mode, and document the rejected combinations. This is
validation of selection semantics, not a new saved representation or support
removal; keep the 0.8.0 floor. Do not rewrite historical schema files or silently
repair old inputs. New arbitrary metadata on the outer selection remains legal
in report readers; the sampling object follows its documented closed shape.

## Alternatives

A public enum with raw count fields still allows invalid counts; a large enum
with per-mode structs duplicates common accounting. A private common struct plus
one enum keeps validated construction and read access small. Continuing with a
validate() call on a mutable public struct would leave internal callers able to
invalidate it after validation.

## Design reviews (five passes)

1. Mode data: Top policy must remain variable; sample alone owns evidence.
2. Existing inputs: report selected=0 and additive metadata must remain accepted.
3. Boundary: present-null sampling is distinct from omission; raw wire parsing
   must preserve this distinction before validating mode.
4. Scope: no new sampler, state-machine rewrite, aliases or compatibility shim.
5. Cost: serialize borrowed fields; preserve ID order and avoid new full clones
   in human rendering. Keep numeric checks in construction, not global types.

## Philosophy evaluation

For this selection boundary, the initial concerns are local ownership of the
selection rules and callers needing to understand valid combinations. The change
should satisfy seven observable diagnostic rows (8.75/10); the design-time
allocation percentage remains unmeasured. Final evidence must verify this, not
assume that introducing an enum by itself improves the design.
