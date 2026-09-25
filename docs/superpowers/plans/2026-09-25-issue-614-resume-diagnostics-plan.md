# Issue 614 implementation plan

1. Commit reviewed design and plan before production edits.
2. Add failing public CLI observations for empty history, completed match, mismatch, eligible resume, JSON/JSONL/human, and normal-run omission; use deterministic max-mutants-limited partial runs rather than timing-dependent timeouts.
3. Add core serde outcome/reason types, optional SessionLoaded reason, and optional RunStarted resume metadata. Carry it through RunState; preserve successful reuse/new-run behavior.
4. Add the no-candidate aggregate history query after existing schema checks and distinguish the lost-eligibility race. Keep corrupt-budget and ownership failures unchanged. Render human text and document precedence.
5. Add session cases with mixed history, older eligible run, increased/decreased budget, old schema/corrupt and locked DB controls; generate finite Lean oracle cases and execute them through real SQLite selection.
6. Review implementation and tests in at least three passes, run related state/session/report/public and Lean checks, exact static CI gates, and full workspace. Commit implementation and review evidence, rebase unpublished branch onto merged parent/current main, publish with gh stack. Root monitors CI at five-minute intervals, merges, then removes worktree.

## Plan self-reviews

1. Failure reproduction: public assertions must observe absent metadata on the old CLI, and prove run IDs/reused termination/exit unchanged. Separate reason correctness from model output propagation.
2. Test matrix: cover no-history, same-complete, unrelated-incomplete, completed-other, budget decrease, precedence collisions and an older eligible match. Keep existing corruption and ownership tests. Lean adapter must use real load with strict corpus fields, not reproduce expected logic in Rust.
3. Execution: no global environment mutation; independent temp roots, bounded subprocesses, controlled Python. One shared Cargo lane; Lean under20s/2GiB and10k heartbeat proofs. Snapshot parent624 is2cf6bad; avoid rewriting published parent and rebase only this Issue before publication.
