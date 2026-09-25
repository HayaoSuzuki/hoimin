# Unresolved fuzz findings

`python_analyzer-mixed-string-tokens.bin` is the original 301-byte input found
during local validation of the bounded CI runner on 2026-09-26. It triggered
a panic in `vendor/ruff_python_parser/src/parser/expression.rs:1635`:
`f-string: unexpected token TStringMiddle at 225..251`.

The four other fuzz targets completed successfully. The runner recorded this
failure and retained the input and logs; the parser issue is still unresolved
at this checkpoint. Minimization was interrupted to save the current work.
The input is kept separately from the normal seed corpus until it is fixed.

Replay from the repository root:

```console
cargo +nightly-2026-07-27 fuzz run --codegen-units 16 python_analyzer fuzz/reproducers/python_analyzer-mixed-string-tokens.bin
```

The finding came from a four-minute local campaign with workflow-style seed
`36158455427` (normalized seed `1798717067`) and an existing local corpus.
The original input's SHA-1 is `7f049fb27444eae05cb4ab9336bb60f89d16ea43`.
Once fixed, minimize this input, add a normal parser regression test, and move
the minimized input into `fuzz/seeds/python_analyzer/`.
