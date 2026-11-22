# Contract and amplification guidance design

## Scope and readers

Implement the next documentation tasks recorded in #1182 and #1186, and summarize
the research commits `1e23185e` and `10f06abe` separately from reusable instructions.
The improvement skill serves agents changing Python tests; the knowledge summary
serves maintainers deciding whether more product machinery is justified.
User authorization covers implementation, verification and PR creation. Broader
research items (other projects, human acceptance and review cost) remain open.

## Boundaries

The existing improvement loop remains the owner of candidate identity, budgets,
fresh baselines and stop conditions. Add a short diagnostic section before the
loop and link one bundled `test-design.md` reference for examples and a review
record. It must work when installed without other skills or this repository.
Do not add a public API, generator, CLI option, executable adapter or report schema.

The reference describes two decisions: whether inputs or observations are missing,
and whether a mock intercepts the relevant behavior. Require an independent
contract, existing-suite comparison, isolated alternatives where useful, observable
outcomes, and explicit unknowns. A caller test need not execute replaced dependency
internals; exact call arguments matter only when the contract requires them.
Keep outcomes, local observations, proposal decisions and human acceptance separate.

The English knowledge page consolidates all seven evaluated research issues,
records sample limitations, and links the unchanged dated reports and raw data.
Follow publication policy: research stays on GitHub; the published development
overview links to it through the existing revision-pinning renderer. No new
publication allowlist or site mapping is needed.

## Design self-review: five passes

1. Scope: next tasks are skill/docs work. Do not claim to complete cross-project or
   human-review studies, and use `Refs` rather than automatic issue closure in PR.
2. Depth: keep execution policy in SKILL.md and substantial decision examples in
   one reference. A separate reference per research issue would repeat contracts.
3. Information hiding: no helper exposes plan construction or changes test argv
   silently. Replan before a broader command; progress still requires identical IDs.
4. Evidence: an already-existing integration test is not a newly gained kill;
   argv-exact rejection is not a behavioral defect without an explicit requirement.
5. Delivery: single-skill installation retains all guidance; maintained English
   instructions do not require reading Japanese historical records.

## Design score

Initial all-inline draft: 6/8 diagnostics, 7.5/10. Interface simplicity and boundary
discoverability fail as examples and history obscure the improvement loop.
Fix: one bundled decision reference plus one knowledge summary; keep the loop
short, with explicit links. Re-score the eight software-design-philosophy diagnostics
after implementation. Technique scenarios exercise guidance without claiming a
statistically demonstrated agent-behavior improvement.
