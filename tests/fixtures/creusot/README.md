# Creusot runner test fixtures

These are snapshots of the **first** successful Task 3 final production proof run
on 2026-10-19, copied from
`.superpowers/sdd/2026-10-19-issue-999-creusot-adoption/task-3-evidence/final/<target>/first`.
The 25 generated `.coma` files are stored once under
`x86_64-unknown-linux-gnu/first/verif`: the corresponding files from all
three target runs were compared byte-for-byte and were identical. Each target
contributes its own `proof.log` showing 59 proved named goals. These files
were copied byte-for-byte; saved `proof.json` strategies were deliberately
excluded. Strategy installation tests read the committed
`formal/creusot/strategies` input instead.

The fixtures exercise the runner's parsers, scope guards and subprocess
orchestration tests. They are never read by `tools/verify_creusot.py`, the
production proof command, or CI's actual Creusot job. That job removes owned
generated output, compiles fresh Coma and invokes no-cache proof replay.
