[監査の要約へ](README.md)

新規Issue: #485 mapping keys、#486 generic scope。共通検証: #489。

以下は調査時の詳細記録。一時artifactのパスは履歴上の参照であり、再現の最低限の手順は対応するGitHub Issueにも保存している。

# Analyzer boundary and cross-feature coverage audit

Snapshot: `623dd808612dbc34775e16814845eec0bc52dff9`; `/Users/hayao/RustroverProjects/hoimin`; debug CLI and CPython 3.14.7. Read-only tracked tree; temporary scripts/results only. Tests below were inspected, not rerun as a suite. Ten targeted source fixtures were run through the real `plan` CLI, their emitted byte edits checked and compiled with CPython, and six fixture baselines plus selected mutants executed. No expensive benchmark run.

Reference shorthand: `R` = `crates/hoimin-cli/src/analyzer/rust.rs`; `T` = `crates/hoimin-cli/src/analyzer/rust_tests.rs`; `O` = `crates/hoimin-cli/src/analyzer/rust/operator_functions.rs`; `C` = `crates/hoimin-cli/tests/operator_function_contracts.rs`. References are exact line anchors at the audited HEAD. A test's existence is evidence only for its named cases; it does not prove the entire row covered.

## Syntax context × operator matrix

| Boundary | Production path | Existing test evidence | Residual gap / status |
|---|---|---|---|
| Binary arithmetic/bitwise/matrix/power vs unary signs/invert vs augmented assignment | R:1613, R:1687 token-role collection; R:211 emission | T:3833 exact AST roles; T:3929 nonoperator spellings excluded; T:3974 token roster; C:344, C:446 CPython numeric/special-method dispatch | Named representatives have runtime evidence, but no shared all-context/all-operator CPython compile corpus found. Deep left-associated BinOp is known #478. |
| Composite `not in` / `is not`, trivia, chained comparisons, remove-not precedence | R:248, R:283, R:1721 operand spans | T:4170 multiline trivia; T:6402 remove-not whole operands; T:3785 general reparse | Reparse uses Ruff, not CPython compilation. Cross newline styles still affected by #455/#469. |
| Ordinary strings/raw text vs f-/t-string expression and format contexts | R:217 skips FStringMiddle/TStringMiddle; R:1687 AST operator roles | T:4742 ordinary strings; T:4759 nested quotes; T:4772 f-/t-interpolation literals and expressions | Existing test covers expressions and literal bytes; no shared CPython-compiled matrix spanning nested format specs × all structural/type producers found. This is a coverage gap, not a confirmed new failure. |
| Function defaults/assert/print/main guard vs full/focused filtering | R:397 retained_by_profile; R:1653 arid ranges | T:4564 focused profile; T:4633/4645 selection ordering; T:4498 bounded full prefix | Existing focused cases do not establish all syntax/binding contexts; preserve full/focused parity in future corpus. Known #461 shows filtering cost happens too late despite correct retained results. |
| List/tuple calls and literals; store/delete/starred/bare generator restrictions | R:1913 collect_call; R:2163/2179 literal candidates | T:2904 exact candidates; T:3074 unsupported shapes; T:2441 yielding values in reparse fixture; T:3757 inner tokens; C:191 parenthesized arguments | Ruff-only fixtures may themselves contain module-level yield. They validate parser acceptance, not CPython program validity. Add valid function wrappers in executable corpus. |
| Method/structure edits, grouped receiver, nested delimiters, comments, trailing comma | R:1913, R:2094 | T:3174/3238/3296 grouped cases; T:3421 CRLF+Unicode; C:127, C:191, C:249, C:276 execute grouped calls/trailing delimiters | Strong representative runtime checks; known #441 closed anchor. No cartesian assertion claimed. |
| Subscript load/store/delete, decimal/nondecimal, slice start/stop/step and zero | R:2094 | T:3676, T:3715 exact neighbor cases | Negative index/slice neighbors intentionally missing (#471); scale of large integer spelling not independently benchmarked here. |
| Raise primary vs cause; exception source/destination identity; ordinary vs except*; risky explicit selection; tuples | R:2195/2221/2227/2251/2320 | T:1447 except* simple/complex; T:1734 raise primary; T:2057 risky reparse; T:1098 tuple exclusions | Parenthesized handler removal known #451; tests exercise many basic tuple cases but no complete CPython compile oracle over nesting/trivia × every exception edit found. |
| Match singleton/sequence/mapping value/OR/capture vs operator function references | R:1753 boolean singleton; R:2446 pattern guard excludes operator_function | T:4068 boolean pattern matrix; T:778 imported callable pattern exclusions | **NEW confirmed:** mapping-key BooleanLiteral and complex BinOp mutations collide with sibling keys. Both AST parsing and existing named boolean-pattern coverage miss CPython compile-time uniqueness. Negative unary sign is separate known #468. |
| Type annotation, type alias, bounds/defaults; nullable add/remove and collection replacement | R:5169 annotation candidates; R:5227 nullable_removal; R:5389 collections | T:4812/4839 PEP 695 positions; T:4908 annotation roster; T:4949 multiline nullable; C:839/879/949 real annotation access/layout | Closed #445 prevents nullable layout regression. Runtime lookup of PEP 695 parameter names is handled differently from annotation lookup; see binding matrix. |

## Scope × binding × consumer matrix

Three separate consumers matter: builtin source/destination pairs (`NameResolutionIndex` R:677), operator-module provenance (`OperatorImports` O:130), and annotation/import flow (`KnownImports` R:2980). Similar language inputs do not imply matching treatment across all three.

| Scope/binding | Existing evidence | Gap / observed status |
|---|---|---|
| Module ordered assignment/import, source and destination shadowing | T:2517/2561/2582 builtin pairs; T:2834 imports; T:5015 annotation rebinding; T:373 operator import guards | Source-order tests exist. No-value `list: object` at module scope suppresses builtin call candidate even though Python still resolves builtin list. Observed false negative; retain as conservative-resolution precision gap rather than independent issue in this pass. |
| Function locals/parameters, lambda defaults/body, nested closure, globals/nonlocals | T:2678 builtin scope; T:5203 annotation scopes; T:5683 lambda defaults; O:537 parameter binding | Representative scopes covered. Cross-product across all consumers is not enumerated. |
| Classes vs methods, private mangling and implicit class names | T:523/554 operator private/implicit names; T:2678 builtin class order; T:2792 exception targets | `operator_function` has explicit class provenance exclusions, while builtin pairs can still trust a metaclass-supplied namespace. See controlled __prepare__ observation below; classify as policy inconsistency pending documented conservative-scope contract. |
| Class list/set/dict/generator comprehension and first iterable vs inner expression | T:669/687/716; C:561 execution | Closed #431 strong regression anchor. Existing #481 comprehension walrus outer-binding miss remains separate. |
| Walrus, match captures, except-target cleanup, star imports, dynamic namespace operations | T:2419 alias/capture; T:2740 wildcard/except/comprehension; T:2875 exec/globals; T:5581 failed match guards | Lean public adapters: `tests/lean_binding_flow_oracle.rs:493`, `tests/lean_exception_match_binding_oracle.rs:635`; private projections T:6577 onward. Formal proofs/corpus only support modeled traces, not unmodeled PEP 695, __prepare__, or walrus placements. |
| Branch/loop back edge/break/continue/try/except*/finally/exits | T:2601, T:5351, T:5520, T:5604; separate `nested_try_oracle_tests.rs`, `multiple_handler_join_oracle_tests.rs`, `except_star_flow_oracle_tests.rs` | Substantial categorized control-flow evidence. No claim of all interleavings or scopes from case counts. Runtime builtin event scans still quadratic (#482). |
| PEP 695 generic function/class parameter names × runtime pair source/destination | R:1175/1194 create scopes without parameter bindings, unlike R:3001 and O:545 | **NEW confirmed:** `def f[tuple](): return list((1,2))` and `class C[tuple]: result=list((1,2))` emit list→tuple where tuple is TypeVar. Original runs successfully; selected mutant raises TypeError. Existing T:4879 covers annotation replacement spelling shadowing only. |
| Unicode identifier normalization vs raw byte spans | Ruff normalized identifiers consumed by all resolvers | Small probe `ｌｉｓｔ = lambda x: 99; result=list((1,2))` correctly suppressed builtin call; `import operator as ｏｐ; result=op.add(1,2)` correctly emitted add→sub and executed as -1. Positive targeted evidence, not full Unicode normalization coverage. |

## UTF-8 / spans / source size matrix

| Boundary | Existing evidence | Residual risk / classification |
|---|---|---|
| Byte offsets versus Unicode scalar columns | R:536 line_and_column; T:4318/4359/6489 | Basic UTF-8 and multibyte text tested; probe checked original byte-slice equality for every emitted candidate. Combining marks/astral prefixes/tabs crossed with all three producers are not represented by a shared matrix found here. |
| BOM at first byte versus internal U+FEFF | T:4338/6510 analyzer unit expectations | **Known #469:** analyzer and shared validator disagree. Direct analyzer tests pass while pipeline fails; add real plan/validator cross-boundary oracle. |
| LF, trailing/no newline, CRLF, CR-only, mixed styles | R:523 scans LF only; T:4318 LF/no-final; T:3421 CRLF structural | Core `candidate_policy.rs:173` checks CRLF+Unicode validator parity. **Known #455** CR-only. Mixed styles need same pipeline matrix. |
| Non-UTF8 and coding cookie | `analyzer/mod.rs:345/493` String::from_utf8 | **Known #480** capability/diagnostic feature. Distinguish unsupported encoding from malformed UTF8; not a newly confirmed bug. |
| Exact span text/path/hash/overflow/error precedence | `hoimin-core/tests/candidate_policy.rs:49/59/70/84/209`; core `candidate.rs:277/286/296/309` line-index/4GiB boundary | Strong component invariants; do not infer analyzer-produced BOM/CR positions consistent with validator. |
| Arbitrary source generation | T:6264 proptest `source in ".{0,4096}"` checks ordering/in-bounds/original | Mostly syntactically invalid input can produce no candidates; there is no nonempty valid-program generation requirement and no compile assertion. Therefore this test is not evidence for syntax-context coverage. |
| Number of candidates and ordered three-producer prefix, zero/max limit | R:CandidatePrefix; T:920/934 zero/usize max; T:4498/4531/4544 bounded outputs | Functional retention behavior covered. **Known #461** eager giant replacements remain; bounded retained count does not imply bounded temporary memory. |
| Many lines vs one long line vs depth/import-state size | T:4210 and T:4238 ignored benchmarks; T:3519 token-lookup operation bounds; T:4378 cancellation | Existing #470 long-line prefixes, #478 deep BinOp abort, #479 annotation import cloning, #482 binding scan. No new benchmark run or threshold assertion from this audit. |

## Confirmed new repro evidence

1. `/tmp/hoimin-analyzer-boundary-probe.py` -> `/tmp/hoimin-analyzer-boundary-probe.json`: 10 source fixtures, actual plan candidates, byte-slice equality, independent CPython compile. New invalid rows: both boolean key edits and both complex key edits. Known #468 negative-pattern case reproduced as a control. `ast.parse` accepts all four mapping collisions while `compile(..., 'exec')` rejects them.
2. `/tmp/hoimin-analyzer-binding-runtime.py` -> `/tmp/hoimin-analyzer-binding-runtime.json`: executes original and selected candidate. Generic function/class originals print `[1, 2]`; list→tuple mutants fail with `'typing.TypeVar' object is not callable`. NFKC positives behave as above.
3. Controlled class namespace: `Meta.__prepare__` returns `{'list': lambda x: 'custom'}`; class body `result=list((1,2))`. Analyzer emits list→tuple; baseline returns `'custom'`, mutant `(1,2)`. O:180 explicitly states a metaclass can supply names without AST Store and excludes class loads. Builtin resolver R:739 lacks that protection. This is a confirmed provenance difference, but blanket exclusions would reduce intended class-body coverage (T:2678); recommend one explicit policy/coverage task, not asserting arbitrary runtime mutation must always be inferred statically.

Issue-ready bodies:

- `/tmp/hoimin-exhaustive-issue-pattern.md`
- `/tmp/hoimin-exhaustive-issue-typeparams.md`

## Recommended coherent coverage work

One durable analyzer-contract test task should own a declarative **valid Python source × candidate family × syntax/scope context** corpus. Each fixture states expected eligible and ineligible candidate locations, passes each emitted candidate through the shared descriptor validator, and compiles the mutated bytes using the supported CPython versions. It should preserve typed differences between missing candidate, invalid descriptor, invalid Python, and valid behavior-changing mutant. This is more useful than adding unrelated counts or claiming the current arbitrary-string proptest covers valid-program boundaries.

Initial corpus must include pattern keys versus values, numeric key equality, grouped/multiline exception/type/structure edits, UTF8/BOM/newlines across token/AST/type producers, PEP 695 source/destination binding guards, and first-iterable versus comprehension-body scope. Runtime assertions should remain small representative protocol/binding cases; not every valid mutant is expected to execute successfully. Extend existing `C:86` support rather than introducing a production Python subprocess requirement. Basic named regressions belong in the two bug issues; broader cross-feature corpus is a separate test capability.

For resource behavior, reuse current issue-specific repro families (#461/#470/#478/#479/#482) in an explicit small/medium/large and structural-depth budget suite after fixes. Keep deterministic operation counters in unit tests and subprocess isolation for stack-abort limits. Retention count alone is not a scale oracle.
