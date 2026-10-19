# SDD ledger — plan: docs/superpowers/plans/2026-10-20-perf-three.md

## Authority and initial checkpoint

User requests #1006/#1007/#1008 implementation throughPR with previous requirements. Main and GitHub both34e3349099ce7fd61838629ee8c0d0b814e65a5d, priorPR1023 merged. Originalcheckout has user-owned untrackedFinder/IDE/Serena files; isolated issue-named worktree leaves them intact. No applicableAGENTS found. All three issues OPEN; currentbody/comments snapshots retained here. No question pending. Read-only design explorations complete; no product edits yet.

Worktree: .worktrees/issue-1006-line-selection-index-1007-shared-manifest-1008-source-index. Baseline release build exit0 in87s, lockedRust1.99,twojobs,incremental0, clean34e33490; fullrelease APFSclone at/tmp/hoimin-perf-three-baseline/release withhashreceipt. Sharedtarget/tmp/hoimin-perf-three-target, existingPythonvenv/cache reused andfuzzgrouprestored. NoCargoowner active atcheckpoint.

## Rulings

- R1 — Ruling: execute the three already specified issues through PR using the established sequential subagent/review workflow without a renewed routine approval gate — user explicitly requests the same conditions and developer authorization persists — cost: routine design interpretations may require review adjustments; any material unresolved user-facing contract remains a user decision.
- R2 — Ruling: keep AnalyzeRequest unchanged and prepare a private All/Restricted prefix-max interval index once per file — it preserves arbitrary raw-range semantics and isolates selection from source coordinates without reusing a path-keyed index — cost: O(R) extra interval storage retains duplicates, and standard sorting is a synchronous phase; measure small controls and document cooperative cancellation.
- R3 — Ruling: place one Arc around private manifest storage inside WorkspaceManifest and remove const from logical_bytes/completed rather than duplicate the scalar — the module owns cheap immutable cloning while worker lifecycle ownership remains separate — cost: const Rust callers must migrate; one control-block allocation/refcount operation is added, and filesystem copy work remains O(files*workers).
- R4 — Ruling: lazily construct only validation location indexes, retaining eager size/decode/hash and strict validation order — existing context lifetime binds the source safely and issue acceptance allows avoiding simultaneous duplicates instead of one-build sharing — cost: successful UTF8 still builds two indexes sequentially and adds once-cell access; arbitrary external validate-before-analyze usage is not globally deduplicated.
- R5 — Ruling: serialize implementation/build/measurement, run focused issue checks then one integration sweep and final automatic plus manual nativeCI — resource limits and review quality match the user's established request — cost: cross-task failures can surface at integration, and nativeCI adds hosted time; repeat suites only for changes/failures.

## Preflight shared-boundary matrix

| Tasks | Produced / consumed boundary | Finding |
| --- | --- | --- |
| 1 / 2 | analyzer selection vs manifest metadata | Independent product modules; sharedCargo and measurements serialized. |
| 1 / 3 | rust.rs CandidateSelection vs LineIndex | Separate data/responsibilities; Task3 must not fold selection into location index or remove Task1 prepared admission. |
| 1 / 4 | operation gates, selection docs, reports | Integration consumes actualcounts and preserves checkedhead; no repeated benchmark claims. |
| 2 / 3 | workspace mutation uses CandidateValidationContext | Manifest handle remains immutable; lazyvalidation preserves strict hash/ID/writeback, no ownership orchargechanges. |
| 2 / 4 | metadata heap gates / workspace docs | Requestedretainedheap separatefromprocessRSS/copycharge; Windowslifetime cases neednativeCI. |
| 3 / 4 | encoding docs / coretests / instrumentedmeasurements | Countrealbuilds, expecttwosequential onUTF8; tempinstrumentation keptoutofproductionAPI; no same-sourceglobaldedupclaim. |

| Task | Internal plan/spec consistency | Finding |
| --- | --- | --- |
| 1 | rawemptyAll, nonemptyinvalidNone; prefixmaxsort andbinarysearch matchraworacle | Consistent; no overflowadjacencyarithmetic, no publicsortobligation. |
| 2 | wholemanifestcheapClone vs isolatedmetadataRED; ArcstoragenoClone | Consistent; removeonlyrequiredconst, preservestructuralequality andslotstate. |
| 3 | lazyResult cache vs eagerbounds/errororder; CLIheap vs permanentcountervisibility | Consistent; no cfg(test) countersassumedvisibleacrosscrates; actualflowheap and temporarycounterpatchseparate. |
| 4 | oneintegrationpass plusfocusedfixes; historicaldocs/catalog/newreportclaims | Consistent; specnewcatalogentryrequired, plans/evidenceoutsideoriginalcatalogscope. |

## Five plan/spec self-review passes

1. Coverage: each issue acceptance item maps to its dedicated task, plus Task4 actualintegration/OKF/PR/nativeCI. #1006 --line/--changed, #1007 failure/cancel/pending lifetime, #1008 actualCLIheap/smallASCII andbuildcounts all explicit.
2. Steps: no productcodewritten; each task first preserves/reusesbaseline, observesrealRED thenchangescode. Behavioralpreservationcontrols are not described asnewfailingtests. Counter/heap assertions meaningful, timing onlyevidence.
3. Types: AnalyzeRequest remainsraw; CandidateSelectionprivate; WorkspaceManifestvalueAPIhandle/privateArc; CandidateValidationContextpublicAPIunchanged/privateOnceLock<Result>. Task3 doesnotdependonTask1indexinternals. Constremoval explicit.
4. Hiddenboundaries: reversed/MAXrange, allproducers, droporders/cleanup, decodedsize/errorprecedence, Latin1/BOM/newlinewriteback assignedowningtests. No new source+unrelatedindex API, mutablecache, compatibility fallback orchangedpayloadbudget.
5. Proportion/resource: four taskswithindependentgates; exploratoryreportsseparatefrombriefs. Baselineclone456MiB logicallyusesclonecopy; max4000files/16workers andtimebounds; serialtwojobs, finalcargo clean. FullsuiteonlyTask4; no repeatednativeCI onintermediatetasks.

## Task status

Task 1: complete (commits ca9afb90..4c906846, review clean).
Task 2: complete (commits 4c906846..e5e33603, review clean).
Task 3: complete at f72134d7 under explicit user D1 +0.1ms practical acceptance; original +0.05ms finding/history retained below. Independent quality approved, I1 addressed by D1 addendum.
Task 4: complete at 1dc37bbb; spec compliant, independent quality Approved, no findings. All planned local checks resolved; exact results below.
Final branch review: Approved at 1dc37bbb, no findings. Final review archival/PR/CI/cleanup: controller work in progress.

- Plan/spec committedca9afb90852c43d1422e543470940991ffaae7a2 with normal4hooks skipped fordocs. Task1BASEca9afb90; fresh implementation agent next, soleCargo owner. Existingpython baseline test intentionally assigned Task1 beforeRED; no local test pass claimed yet.

- Task1 checkpoint: existing hoimin-python lib baseline423passed/3ignored. Actual old predicate measured Nqueries and1M/4M/16M/64M membership comparisons for N1000/2000/4000/8000; four new complexity gates RED, selection/control cases GREEN. Report/evidence still pending.

- CI handoff checked: ci.yml Rust job runs workspace, CLI reference, and performance gates; non-linux-ci.yml manual workflow has no inputs, dispatch on final branch ref, includes macOS14/Windows workspace/performance and native lifecycle/resource checks. It is a final check, not per-task repeated work. No workflow change currently required. Task2/3 briefs prepared with exact global constraints, preserved baseline, discovery references, and distinct report/evidence paths.

- Task1 progress: prepared selection owner implemented; reportedGREEN selection59, payload27, cancellation30, publicCLI3 (1000selectors, IDs/order/truncationequivalence, noncontiguouschanged). Finalreport and releasepairedmeasurements pending; no final approval yet.

- Resource checkpoint: sharedtarget2.0GiB, preservedbaseline455MiB logical, task1scratch7.1MiB, free116GiB; no cleanup needed during active comparisons. Task1 paired release preliminary results reported: N8000 sparse26.038→4.062msmedian, +64000requestedpeakbytes, allAPIdigests match. FullCLI timings mixed, including9.257→11.657msmany-line; reportmustexplicitlyretainthislimitation, notgenericno-regressionclaim. Finaltests/hooks/reportpending.

- Task1 finalreport reviewed: all15self-reviews, baselineRED/GREEN, exactreleasehashes/outputs and mixedCLItimings retained. HEAD4c906846e4515735d7e89c17a2b374483ba299ff clean. Third normalcommit attempt passed4hooks after test-only assert/fixture lint fixes with focusedreruns. Review package149059bytes generated over fullTask1range; independentreview dispatched, no currentCargoowner.

- Task1 review spec✅/qualityApproved, nofindings. Controller resolved evidence-only⚠ by opening rawRED actual1M/4M/16M/64M failures, payload27GREEN and4Passedhooklog, rehashing immutableoriginalbinary toknownreceiptSHA. Existingpayloadcontracts unmodified and focusedtestssuccessful; integrationwillretainreceipts. Noopenreviewitems.

- Task2 realRED reported beforeproductedit:4000files×16clones retain7510400bytes (>16KiB),1000files×4actualworkers474312bytes (>288KiB). Meter selfchecks live/realloc/release/peak; outputhandlespreallocated; semanticdigest/chargechecks outsidewindows. Implementationandlifetimeverificationongoing; no completionclaim.

- Task2 initialGREEN:4000×16clonehandles additionalLIVE0/PEAK0 vs7510400RED;1000×4workersLIVE712/PEAK66427 vs474312RED. Workspace240passed/7preexistingignored (preservation238/7before). Newpause/abortwaiter casechecks blockingcopyfinalplanownership andWeakfinalrelease; siblingfailure/charge,plan-drop/resetpassed. Release144observations planned24workloads×3pairs×2versions, first30in14s; optionalRSSomitted, retainedheap/timing/digest/charge separatelyrecorded.

- Task2 measurementcheckpoint141/144; fixed600soverall/120sperprocess bounds, noexpansion. At4000files/16workers observedafterLIVE2696/PEAK68411bytes; rawdigest/logicalbyte/charge/snapshot+workerpayload agreement checked. Final3samples andscopedfailure/reset/cancel/hookvalidation remain.

- Task2 final report read: all15stagepasses,144/144pairedobservations in368.76s, before/afterdigestandchargeagreement; finalworkspace240/7ignored,recovery24,heap5,materialization9/1ignored,cancelrelease1. Counts overlap, notdistincttotal. All4normalhooks passedfirstattempt; cleanHEAD e5e33603a4ed6a269d4131fbb6dbe99a409de05c. FullTask2package161746bytesgenerated; independentreviewactive. NativeWindowscases pendingfinalCI, optionalRSSnotmeasured; FS timingsmixedhonestlyreported.

- Task2 independentreview spec✅/qualityApproved, nofindings; revieweropenedrawRED/GREEN/hookswithoutretesting. NativeWindowsandfinalintegration⚠ areplannedTask4/controllerchecks, notmissingTask2work. Noopenreviewitemsorparkedminors.

- Finalization sequencing note: archive task/finalreview reports andrulings beforefinalpush; runautomaticandmanualnativeCI againstthathead, thenupdatePRbodywithrunlinks/resultswithoutanotherdocumentationcommit. This avoidsclaimingnativeCIonadifferentfinalhead. CIobserveddefectswillgetscopedfix+reviewandfreshaffectedchecks. No merge/release.

- Task3 realRED→GREENcheckpoint: unusedcontext actualconstruction1→0; firstpostconstructionvalidation0→1. Contextrequestedheap2097232→64bytesatN262144. ActualCLIdiscoverypeak4722483→2625315bytes, N=1048576: 18877712→10489088 bytes; eagerlyretainedindexsensitivitycontrolstillviolatesgate. Focusedcoreunit21,coreintegration/property35,CLIencoding18passed;Latin1heapforcesvalidation. Releasepairedmeasurements/privateactualconstructor-dropinstrumentationpending.

- Finishing skill rechecked for finalization: useralreadyselectedpush+PR, so no redundantmenu/approval. PRoptionpreservesissueworktreeandlocalbranchforfeedback; cleanupremovestask-ownedbuild/scratch andonlythisSDDworkspaceafterarchival, notactivePRworktree. FinalCIheadwillbechecked; no repeatfullsuiteforreport-onlyarchival.

- Task3 smallcontrol concern: initial3-pair smallASCIIplan5.244→8.726ms, pipeline0.255→0.285ms; tinyplan4.855→4.825ms, smallplanrequestedheap165862unchanged, contextstack160→168bytes. One boundedconfirmation justified byconcreteunresolvedregression:12alternatingpairs×tiny/small×pipeline/ordinaryplan×2versions=96childinvocations,≤25seach, afterbuild/instrumentationstop. Originalsamplesretained; nopickingfavorablebatch, nomorebatchesplanned. Noacceptancerelaxation; persistentmaterialregressionmustbereportedbeforearchitecturalchange. Ordinarybefore/afterwholeCLIincludesTask1/2confounding; isolatedAPI/eagersensitivityandactualconstructor-dropcountsprovideTask3-specificevidence.

- Task3 instrumentation observedactualconstructor/drop: pipeline/discovery/plan builds2→2,maxlive2→1; runbuilds3→3,maxlive2→1; context1→0. Exactproductioncandidate.rsrestored. Boundedconfirmationcomplete: smallpipeline medians0.298958→0.299271ms (~0.1%); smallordinaryplan5.510→5.850ms (+6.2%, before4.922–6.685/after5.099–6.503). Pairedsmallplan after−before8positive/4negative, median+0.236521ms,mean+0.245913ms,range−0.634333…+1.049833; pipeline6positive/6negative,pairedmedian+0.003125ms. Tinyplan6/6,pairedmedian+0.032687ms. Initial66%difference didnotpersist, residualsmallCLIuncertainty/160→168contextstack explicitlyreservedfortaskreview; nogeneralno-burdenclaim, nofurtherbatches.

- Task3 finalprecommitfocusedreport:300distinctpassed/12preexistingmanualorfixtureignored; registrycheck29shapes (notfullgatecount). OrdinaryreleaseafterinstrumentationrestorationexactSHA644f8e41c502b6bc6ddebb45cf9a00e1d85925c95585b8ddee0e281cc284274a matchespreservedmeasuredcandidate. Hooksfoundtest-onlyformat_collect and3oldLineIndexunitcallswithremovedignoredargument; implementerfixingwith13flowcaseandfocusedline_indexreruns. Productbehavior/measuredsourceunchanged. Sharedtarget3.1GiB,scratch7.8/5.4/84MiB,free114GiB.

- Task3 finalproduct5a1d6861 +receipt952ab52780b181ae8de316e4b1f45988b5d0d1b0, clean. Finalreportreadall15passes/304distinctGREEN12ignored,4normalproducthooksPASS(aftertworesolvedtest-onlyfailures); receiptcommithooksskipdocs-only. Full2commitpackage738332bytesgenerated. Reviewerexplicitlyassesses small-ASCIIcriterion/evidence, no pre-judgedseverity; functional/heapclaims separatefromunqualifiedtimingacceptance. No currentCargoowner.

- R6 — Ruling: operationalize the issue's small-ASCII no-burden criterion as a prespecified task-local experiment: for each existing tiny/small fixture, the exact one-sided95% distribution-free upper confidence bound of the paired median ordinary-plan elapsed difference must be≤+0.05ms; requested whole-plan peak/retained heap must not increase. Compare e5e33603 production source against Task3 production only, same compiler/profile/path/env/fixtures,512 alternating pairs per fixture in one fixed batch, no sample deletion or optional stopping. This resolves the review's causal/evidence gap rather than repeating cumulative main→all3 samples. Strict zero runtime difference cannot be established with finite noisy process measurements;0.05ms is about1% of observed5ms startup. Cost: this practical criterion can accept a small positive median difference and is only a finite-workload/confidence statement, not a universal no-regression guarantee; context+8B and all older adverse observations remain disclosed. A failed/inconclusive bound requires scoped diagnosis/fix, not silently relaxing this threshold.
- Task3 reviewI1 confirmed as an evidence gap; functional/heap/design qualityapproved. No product correctness defect identified. R6 fixes the experiment design before new observations; no new architecture or runtime API authorized by this ruling. Original implementer resumes, no concurrentCargoowner.

- R6 fixed experiment finished:2048 ordinary timing observations +12 heap observations in51.579s. Exact one-based upper order rank276 has coverage≈0.957657. Tiny upper−0.019834ms passes; small upper+0.067791ms exceeds+0.05ms, so I1 remains open. Small pairedmedian+0.021354ms,mean−0.020568ms,signs268positive/244negative; neither a>0.05ms median regression nor non-regression is established. Requested whole-plan peak and retained deltas0 in all6pairedheapobservations. No deleted/outlier samples, no new batch, no threshold relaxation. Source restored; implementer finishing evidence-only archive/hooks.
- User clarification D1 pending via request_user_input_async: accept practical+0.1ms upper-bound tolerance and adopt current implementation, or retain+0.05ms and investigate/improve. Question identifies issue#1008's no-small-input-burden acceptance, exact observedmedian/bound and unchangedCLIheap. The+0.05ms initialthreshold was explicitly identified in commentary as controller's choice. Elapsedtime is NOT acceptance; do not markI1 resolved/Task3complete or unconditionallyclose#1008 before response. Independent frozen-source integration/draft work may proceed after currentmutation/Cargoowner hands off, perdeveloperasync-progress instruction; affected checks must rerun if subsequentproductchangesoccur.

- R7 — Ruling: after Task3 evidence-only handoff, allow Task4 frozen-source integration/document work while user clarificationD1 is pending, without markingTask3accepted orclosingI1 — developer instructions require useful independent work during asynchronous clarification, overriding the skill's normal sequential acceptance gate. Cost: if the user chooses further product improvement, affected integration checks must be repeated; no competing implementation/Cargo owners or premature completion claims are allowed.

- Task4 integration dispatched at frozenHEAD f72134d76d28d87cd728305ed224f298df41af4a, soleCargo/productowner; Task3fix1 evidence re-review runs read-only concurrently. D1 remains unanswered; I1open. Task3fix1evidencecommit contains no product/test/registrychange.
- Remote pre-PRcheck: allthreeissuesOPENunchanged; noopenPR. main advancedone dependency-onlycommit dd01e6c61dd7cef49efaf4427a662aa12e98008e (PR1024, Hypothesis6.170.0→6.170.1 inpyproject.toml/uv.lock), productbaseunchanged. Keep measurement/testbase34e33490 andcurrentlockstable; no unnecessaryrebase duringintegration. PRmergeCIchecks latestmainmerge, nativeCIchecksfinalbranchhead; recordboth accurately.

- Task3fix1re-review: I1NOTADDRESSED underinitial+0.05mscriterion; no newbreakage/outofscope. Reviewerindependentlyverified observations/pairing/rank276/heap/provenance/receipts. Originalverdictpreserved in task-3-fix-1-review.md.
- D1 EXPLICIT USER DECISION RECEIVED: user selected「+0.1 ms以内を許容して採用する（推奨）」in response to question disclosing smallpairedmedian+0.021ms, one-sided95%+upper+0.068ms,unchangedCLIheapandinitial+0.05mscriterionnotmet. Accept currentimplementation underpractical+0.1msupper-boundtolerance andnoCLIretained/peakheapincrease. This isapost-observationuseracceptancedecision, NOT apre-registeredcriterion ornewmeasurement. Tinyupper−0.019834msandsmallupper+0.067791ms meetnewtolerance; sixheappairsdelta0. PreserveinitialR6criterion/failureandalladversesamples; nouniversalno-regressionclaim. Task4updatescurrentdesign/docs; Task3reviewerproducesacceptanceaddendumwithoutretesting. Sourceunchanged, integrationcontinues. R7pending-decisionrestrictionnowlifted; noaffected-codechecksneedrepeatfromthisdecisionalone.

- Task3D1acceptanceaddendum: I1ADDRESSED underexplicituser+0.1mstolerance; bothverifiedboundsmeetandallheapdeltas0. Original+0.05msNOTADDRESSEDverdictunchanged. No remainingTask3finding; taskaccepted atf72134d7withD1decision. No newmeasurement/productchange. Addendum task-3-d1-review.md readyforarchive.

- Task4 complete: docs/evidence commit1dc37bbbfdf2e9b8c00db8fe51667fa0348e5a92, clean, no product edits. Workspace3430/33ignored; corecontracts705/2; CLIcontracts2228/28; reference7; fmt/clippy/Ruff/ty pass;47/47gates. FullPython1197pass/11skip/4failure caused by sandbox ps PermissionError; controller authorized scoped normal-environment retry and4/4pass, aggregate1201/11skip (NOTonegreenfullrun). Originalfailure andenvironmentproof preserved. No suite repeat orsourcefix. 60selfreviews,113archivehashes verified; normaldocscommithooksskippedcorrectly.
- Task4 independentreview: speccompliant/qualityApproved; noCritical/Important/Minor. Reviewerrecomputedmembership,hashes,testcounts,11OKFsourcehashes and47gatestatuses. Controllerownsremainingpublication/nativeCI/cleanup.
- Finalwholebranchreview34e33490..1dc37bbb: ApprovedforPR/CI, noCritical/Important/Minor, nocombinedfixwaveneeded. ActualCLIordering/errorpriority/charge/ownership/encoding/lifetimes andall substantivecode/tests checked; independentrawmeasurement/provenancearithmetic andintegrationlogcountsverified. No testreplay orproductchanges. Fullreportfinal-review.md archivedbeforepush.
- Finalreviewdeclineditem1 — Controllerruling: accept exclusion of universal latency/RSS/physical-disk/zero-costArc/hardcancellationguarantees, consistentwithR2–R6andissuecontracts; cost isfiniteworkload/requestedheap/cooperativecancellationevidence only. Retainallobservedincreases andtiminglimits; no broadenedguaranteeclaimed.
- Finalreviewdeclineditem2 — Controllerruling: accept exclusion ofglobaldeduplication/arbitraryexternalcallorder/one-constructionguarantee, consistentwithR4; cost is2sequentialUnicodebuilds (actualrun3),externalcallerscanoverlapindexes. ActualCLImaxlive2→1isverified.
- Finalreviewdeclineditem3 — Controllerruling: keepinheritedOKFrepair/externalURLfragmentvalidation/unrelatedissues/formalextensionsoutsidethis3-issuescopeunderR1; cost isexistingdocumentreferencegapsremainitemizedandarenotcertified. No newlyintroducedlocalreferencefindings. NativeplatformchecksareNOTexcluded: executeautomaticPRCIandmanualWindows/macOSnext.
- Finalarchive ismechanicalreports/ledger/receipts/manifest only, no product/test/tool/workflow/proofchangesafterfrozenintegration. KeepPRworktree/branchforfeedback; taskownedtarget/baseline/scratchandonlythisignoredSDDdirectorywillbecleanedafterevidence+CI. No merge/release.
