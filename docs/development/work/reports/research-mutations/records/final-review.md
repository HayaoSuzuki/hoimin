# Whole-branch review of the research mutation operators

**Readiness: with fixes.** All nine issues have implementations, and all 22 new IDs are default-enabled and individually excludable. Five Important findings remain at 6df6dcb7. They affect exact set-receiver identity, root regex mode coverage, malformed regex eligibility, repeated referenced-literal work and cancellation while building call facts. No Critical finding.

Reviewed 4cf630f977dc266c6633969b57d0f9f251bb91d8 through 6df6dcb7deec71946845043408d4ce256dc55044. The review used the supplied package, root design and plan, domain specifications, implementation reports, integration brief and ledger. Focused probes used the built CLI, temporary subjects and CPython 3.14.9. No passing suite was rerun.

## Strengths and specification coverage

- The catalog appends IDs 104–125. The default planner fixtures name every addition and compare the remaining candidates after excluding each one. Selection, scoring, stable IDs and source encoding still use the common planner. The public mixed-family tests cover line, symbol, intersection and bounded rank order.
- Cleanup, match dispatch and adjacent statement exchange have executable normal/abrupt, guard/fallback and side-effect-order oracles. Header-based cleanup retains the original suite bytes. In the focused profile the statement exchange checks each participant against the arid-region index.
- Escape removal, argument exchange and result defaults preserve argument evaluation and direct-await execution under the documented contracts. The runtime fixtures observe exceptions and side-effect ordering. Collection processing covers comprehension clauses, lazy filters, set receivers and iteration-based counting. Precision coverage includes rounding direction, digits, Decimal quantum, context and intermediate operations.
- The regex scanner separates Python pattern semantics from Python literal spelling. Scoped overrides, complements, captures, match regions, escaped units, implicitly concatenated tokens and both supported interpreters have executable witnesses. Selection precedes ordinary edit rendering. The shared-pattern cache addresses repeated parsing, including zero-result consumers.
- Two authentic 0.14 manifests replayed with the final CLI, including the old 85-operator default selection and its original seven candidates. Five survivors became five kills under a stronger arbitrary verification command, with the saved identities, source and plan digests preserved. The final replay log records passing baselines and improving progress.
- The root design and plan each record five substantive self-reviews. The domain implementation reports separately record implementation and test passes, corrections and ensuing checks. The integration report has five of each. Independent task reviews add separate evidence. The named parameter rows, actual CPython observations and bounded properties go beyond parse-only examples. The heap matrix has the test-granularity exception below.

## Important findings

1. **An explicitly replaced module variable is still trusted as an exact set.** At crates/hoimin-python/src/analyzer/rust/collection_processing.rs:143 the stored initializer is enough to accept a method on a name. The module initializer proof at crates/hoimin-python/src/analyzer/rust/local_initializers.rs:72 consults lexical writes and wildcard state. The namespace invalidation at crates/hoimin-python/src/analyzer/rust.rs:2582 recognizes direct globals() calls but misses explicit access through a function's __globals__. In a temporary module I defined Custom.union to return 'custom', defined marker(), then executed the following.

       s = {1}
       marker.__globals__['s'] = Custom()
       out = s.union({2})

   CPython returned 'custom'. The planner emitted union → intersection. Custom has no intersection method, so the candidate introduces an unrelated AttributeError at an unproven receiver. Replacing the write with globals()['s'] = Custom() produced zero candidates. Removing the write gave the genuine set positive control and one candidate. The call and regex families already recognize __globals__ as a module-identity hazard. Feed that fact into proofs that depend on module-variable initializers, including access from function bodies. Cover the ordinary module set, the reflective write, and a local set whose binding remains provable.

2. **A valid root declared both inline and externally cannot be mutated.** At crates/hoimin-python/src/analyzer/rust/regex.rs:507 the emitter returns immediately when both supply ASCII/Unicode. The negative case at crates/hoimin-python/src/analyzer/rust/regex_tests.rs:217 pins the omission. With regex_ascii_mode alone a public plan emitted nothing for re.compile(r'(?a)\d', re.A) or re.compile(r'(?u)\d', re.U), and emitted one candidate for the external-only control. CPython reported flags 256/32 and rejected/accepted U+FF19 in those two modes. Toggling the root requires updating both spellings, and per-atom scoping cannot change Pattern.flags.

   The earlier regex domain specification explicitly says to suppress changes requiring disjoint edits. This is a **design limitation carried into the implementation**. The user's complete-mode requirement, the root's precedence for full scope and observable flags, and the public operator description justify fixing it. A single candidate can cover both spellings while copying intervening call text unchanged. Compose the local literal or its projected use with all external terms for that root, in source order even for reordered keywords. Preserve other flags, comments, scoped modes, independent consumers, selected-start semantics and the existing saved edit representation. Pin both directions with real CPython acceptance and flags.

3. **Unknown flags admit method mutants for provably malformed literal patterns.** The direct-call fallback at crates/hoimin-python/src/analyzer/rust/regex.rs:425 and the compiled-receiver path at regex.rs:337 only decode the Python literal in this case. A public plan for re.search('(', s, flags) emitted search → match. For re.compile('(', flags).match(s) it emitted match → fullmatch and match → search. A lone opening parenthesis cannot compile in either ordinary or verbose regex grammar. These mutants can never exercise the selected matching behavior; exception-catching callers can report meaningless survivors. Retain method mutation for dynamic patterns or known literals whose validity depends on the flags. Suppress it when every supported grammar interpretation fails. Check the direct and compiled paths with the same validity proof and include a verbose-sensitive positive control.

4. **Linear input can force quadratic replacement copying for candidates already guaranteed to lose.** At crates/hoimin-python/src/analyzer/rust/regex.rs:475 the per-use counter starts afresh. At regex.rs:492 each eligible descriptor is projected to a complete literal string by regex/literal.rs:313. Only after that allocation can crates/hoimin-python/src/analyzer/rust.rs:368 reject it by its source start. For a shared M-byte literal with at least two shorthand atoms, N increasing use sites, regex_character_set alone and K=1, the two retained-prefix slots are already filled at the first use. The loop still constructs two M-byte replacements at each later use. Reading the file and checking N call sites is necessary. Copying these later literals costs Θ(N × M) bytes of work while the returned payload stays Θ(M).

   A bounded public probe used four leading shorthand atoms, literal suffix lengths 8,192 and 65,536 and 1 or 128 distinct uses. All four results returned one candidate and truncation. Runs took 0.05–0.08 seconds; this is a functional witness, not a timing regression claim. The allocation count above follows from the current loop and at_reference. Expose a conservative cheap rank/admission check or a lazy replacement callback. Bypass rendering only when the candidate cannot enter the prefix, maintaining overflow/duplicate accounting and scanning for later **better-ranked** edits. Exercise the existing generated-byte counter across many positive-result references, compare the complete bounded prefix against unlimited planning, and retain the existing zero-result/cache and failed-sink cases.

5. **The call-signature preprocessing pass ignores the supplied cancellation callback.** At crates/hoimin-python/src/analyzer/rust/optional_keywords.rs:92 the new call-contract path invokes from_module_with_names. At crates/hoimin-python/src/analyzer/rust.rs:1364 that function constructs the builder with a constant false callback. The imported-name pass and full lexical walk between lines 1365 and 1373 therefore finish before cancellation can be observed in the later hazard walk. This also affects a request selecting only call_result_default when a module contains an eligible definition. A large module can overrun the cooperative analyzer deadline through two complete traversals. The general helper predates the new IDs, but the branch expands the work routed through it.

   Pass the actual callback through both name collection and lexical indexing, returning cancellation instead of publishing partially collected facts. Add a bounded callback-count test that trips within this portion of signature preparation rather than before it or in the subsequent hazard scan. Preserve the ordinary no-cancellation result and existing optional-keyword eligibility.

## Minor findings and parked items

- **One heap-test invocation runs three independent selectors.** At crates/hoimin-cli/tests/regex_heap.rs:45 each shape invokes three independent discovery operations inside one loop. Each has its own measurements and assertions, so failure in one skips the others. For the requested one-case-per-invocation convention, give each shape/selector pair a named row and keep the common fixture and measurement mutex. The comparative table can still use the same inputs and K. Multi-input acceptance bitmaps in the runtime contracts observe a single mutant and can stay together.
- **Redundant suite entry.** crates/hoimin-python/src/analyzer/rust.rs:3551 forwards all arguments to statement_order.rs:114. Keep one named visit_suite entry in the order module during cleanup. The wrapper changes no behavior and needs no separate merge gate.
- **Fixture Git warning.** The warning at .superpowers/sdd/research-mutations/task-5-workspace-tests.log:839 concerns a temporary src/calc.py checkout under native Git line-ending settings. The target passed. Close as informational. Normalization of fixture-local Git settings can accompany a future change to that fixture.
- **Previously parked Task 1 and Task 4 work is addressed.** The final control contracts exercise a cleanup return or raise overriding a pending exception. The order module owns its directive cache. PatternSite owns semantic scope rendering and the literal writer handles quoting, parity and separation. The generated runtime checks have explicit arrange/act/assert sections and the usage guide describes complement widening and both neighbors of a word boundary. These items need no further source changes.

## Verification disposition

The initial full workspace run reported 17,361 passed, 41 failed and 45 ignored. The two integration regressions and ten fixture failures have targeted green receipts. The full CLI contracts run after repair reported 8,298 passed, 29 failed and 30 ignored; the log footer names only the CLI library. Every integration target passed. All 29 remaining names match the documented native-workspace failure set. Ten failed at the same stages on isolated base 4cf630f9. The remaining 19 have matching native failure signatures and unchanged analysis-independent ownership/lease paths. This supports treating them as inherited for this feature gate. It does not independently reproduce those 19 at base.

Core contracts, strict workspace lint, formatting, generated reference, site, frozen Python tests and the two supported CPython contract runs have green receipts. The old/mixed replay evidence is from the final default CLI binary. The initial default workspace suite exited nonzero, and the final CLI contracts suite also exited nonzero. No full final default workspace rerun is claimed.

## Design diagnostic

**5/8 rows supported, 6.25/10.**

| Diagnostic | Result | Reason or change needed |
| --- | --- | --- |
| Each module's purpose can be described in one sentence | Pass | Syntax scanning, source literal mapping, stable initializer lookup and structural exchanges have distinct responsibilities. |
| Interfaces are simpler than their implementations | Pass | Consumers use selected edit ranges and replacements while the domain modules own parsing and evaluation details. |
| Implementation changes can stay behind the interface | Fail | Whole-literal projection happens before common rank admission. Give admission a lazy or conservative metadata-only entry so changing a rank policy does not require domain-specific assumptions. |
| Interface comments describe the abstraction | Pass | The source mapper, evaluated arguments, delimiter limits and reaching-initializer contracts explain the guarantee callers rely on. |
| Design discussion participates in reviews | Pass | The review records contain concrete revisions to scanner ownership, literal mapping, binding cost, directive lifetime and prefix behavior. |
| Each module hides its important decision | Fail | Module identity after explicit dynamic namespace access differs between callable and initializer proofs. Centralize the relevant hazard fact and use it wherever a proof depends on that module namespace. |
| Boundaries are understandable from contracts | Pass | The operator guide, domain specifications and module comments identify eligibility, ownership and execution effects. |
| 10–20 percent of effort invested in design improvement | Unverified | Refactorings and measurements are documented. No contemporaneous time breakdown establishes this percentage. Capture the split during future work when a numeric claim is required. |

## Declined to judge

- Whether each of the other 19 native Windows failures reproduces individually at base. Only ten base probes were run. A separate native workspace investigation would be required for case-by-case verification.
- Actual POSIX process-group and shell behavior on a POSIX host. The supplied final run used native Windows and controlled doubles, with platform-gated real POSIX fixtures retained.
- Replay of every possible historical 0.14 plan. The preserved examples establish actual explicit/default replay; an exhaustive historical corpus was unavailable.
- An exact numeric design-time fraction. The recorded changes establish design investment, but elapsed design and implementation time were not tracked separately.
- Full behavioral equivalence for arbitrary Python regexes, callable identities under arbitrary runtime mutation, and arbitrary annotated return values. The design explicitly uses bounded static proofs and declared contracts. I did judge explicit source-visible rebinding, known malformed syntax and the accepted grammar examples, including those above.

## Verdict

**Implementation/spec: material issues remain. Design: 6.25/10. Ready to merge: with fixes.** The native Windows disposition and the two parked integration observations do not block these nine issues. Resolve the five Important findings and rerun the affected independent review, admission/selection checks and runtime witnesses before marking the whole-branch gate complete.

