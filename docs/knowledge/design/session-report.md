---
type: Contract
title: session所有権とレポートの検証
description: 保存・復旧の権限、スキーマ移行、結果の生成側と読取り側の検証責務を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-1010-wire
  resource: ../../../crates/hoimin-core/src/report.rs
  revision: 6ea967d6b1682596d6a2c0dda3d1c458f9ce5e6a
  working_tree: modified
  sha256: b000930d1bd41fe60dd9dc9e459c3947cb667711ceec450c0d8673c95904f993
- id: issue-1010-human
  resource: ../../../crates/hoimin-cli/src/report/human.rs
  revision: 6ea967d6b1682596d6a2c0dda3d1c458f9ce5e6a
  working_tree: modified
  sha256: bc69723f31cb4e44a565471939a52377f48b25c3faf807e461509679675945dd
- id: issue-1015-wire
  resource: ../../../crates/hoimin-core/src/report.rs
  revision: 8720b41a0d08f375d98bda7cfa6202be779f3226
  working_tree: modified
  sha256: c89bddbf335142d9586ecbc3e155893c273af3718fb223e3975e66e97954a013
- id: issue-1015-compatibility
  resource: ../../compatibility.md
  revision: 8720b41a0d08f375d98bda7cfa6202be779f3226
  working_tree: modified
  sha256: 5e7c0deae74941aa9651590a7b1e6d7465a75fa8e2063090b7398c30214aafa9
- id: issue-1004-reader
  resource: ../../../crates/hoimin-cli/src/artifacts/run_report.rs
  revision: 6d78789eca05fad577b2a8dcce83077f9389ee06
  working_tree: modified
  sha256: 55fc1aab381c73d4adbb1de145d6402cb7e1fc0d0c6f3124cede9d8fe3d61fd3
- id: issue-1004-config
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: 6d78789eca05fad577b2a8dcce83077f9389ee06
  working_tree: modified
  sha256: 6c0aa32efc26a872a6f5c3aaf32c7b9cfad9cb5ae58d4aa83a9ce5c26461fba4
- id: issue-1002-human-text
  resource: ../../../crates/hoimin-cli/src/human_text.rs
  revision: b02ee7d51709de664ee88db68abca34025db3487
  working_tree: untracked
  sha256: 00322fd04c933a8ee575787e2f65bbb641059fd043a8493d76beca57a6ccf357
- id: issue-1002-human-report
  resource: ../../../crates/hoimin-cli/src/report/human.rs
  revision: b02ee7d51709de664ee88db68abca34025db3487
  working_tree: modified
  sha256: 996c546875b4f7563cb35df742d4cd23aad5dc6e83f4c8d5a6aa69aedad3d4c1
- id: issue-1002-human-tests
  resource: ../../../crates/hoimin-cli/tests/report_handler.rs
  revision: 3d9e33d178a451e39e182495f4a5bb3a4c5ebfa0
  working_tree: modified
  sha256: af99502af3e8b1dab50efbd3ce2c9b0e4ae1db4dfebe4a5692f407ceec524c93
- id: issue-992-identity
  resource: ../../../crates/hoimin-core/src/candidate.rs
  revision: 50b9c089ca73aeca2205636a2a9e1823cd244d39
  working_tree: clean
  sha256: 0bec0dbf0fa391f301a1bec2f41b783732ac4248676adb055113f7370affa54f
- id: issue-992-reader
  resource: ../../../crates/hoimin-cli/src/artifacts/run_report.rs
  revision: 50b9c089ca73aeca2205636a2a9e1823cd244d39
  working_tree: clean
  sha256: 1de6d012acebb98f0e26abcc19a5b372c2cd20751f4f988a9e9c2a4518c26dba
- id: issue-940-policy
  resource: ../../../crates/hoimin-core/src/resume.rs
  revision: 465ff928c1f2ceb9f9b8f8084465475e45799ee8
  working_tree: modified
  sha256: 53f4c43a3777542ca8e70ef8c7547bbd677298159fbda01e177919bdb886623a
- id: issue-940-review
  resource: ../../maintenance/2026-10-16-issue-940.md
  working_tree: untracked
  sha256: 5757dba4774fa7dd8c558c8490efbde5e24b62cc864078f49168b3141c9d4d96
- id: issue-939-reader
  resource: ../../../crates/hoimin-cli/src/artifacts/run_report.rs
  revision: 465ff928c1f2ceb9f9b8f8084465475e45799ee8
  working_tree: modified
  sha256: 1fe4df8f53caeb995059243a9139a0b0eeded7f24c421d0eb5624bdbaa78b911
- id: issue-939-compatibility
  resource: ../../compatibility.md
  revision: 465ff928c1f2ceb9f9b8f8084465475e45799ee8
  working_tree: modified
  sha256: 789f97d7475c4fbabdc81bb7aa735674429711e8bc00253872658f2d149527c3
- id: issue-939-verification
  resource: ../../maintenance/2026-10-16-issue-939.md
  revision: 0b29290f26055f49b4754efe68928ace0366b613
  working_tree: modified
  sha256: fc49c4d781f1778b5ac857c2e965d00f8a038f84e5850338ae5279b1aed294f9
- id: issue-923-config
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: 7c3a9105c432ec1e0ba2483ada42b10133d071a6
  working_tree: modified
  sha256: 1c0680d4872c468f4c1c3fb6433b589da1f7752bc6f10b70d71eba876bcefb08
- id: issue-923-compatibility
  resource: ../../compatibility.md
  revision: 7c3a9105c432ec1e0ba2483ada42b10133d071a6
  working_tree: modified
  sha256: b060c02db7f6e3adc035a61f2d954caa387674b64cbb273269587e170baf0c1c
- id: issue-916-budget
  resource: ../../superpowers/plans/2026-10-15-issue-916.md
  working_tree: untracked
  sha256: 9fc14e91942ef296cbf2f92c1e40941ce36f4676f6bac27d534a3687484eb9ee
- id: issue-899-changed-selection
  resource: ../../superpowers/specs/2026-10-15-issue-899-design.md
  revision: 7b6cb19ba0181759f6b5726f985c5fd7700d313e
  working_tree: clean
  sha256: fa68b3e4aa56ab0ee3dd19f818a0b932a033dce550dbc869f956c261578b501f
- id: issue-899-changed-review
  resource: ../../superpowers/reports/2026-10-15-issue-899-review.md
  revision: ad65b8ce55b0dabc59712775e362d5d614b27559
  working_tree: modified
  sha256: 50a53fa428a46289ede33f60ae1bcc342f8cfa3b2a03e37ebc5ad127ff5444bc

- id: issue-896-config
  resource: ../../../crates/hoimin-core/src/config.rs
  revision: e7e3cae8c4e048ee77d2bf644b47da1792211cbe
  working_tree: clean
  sha256: b8d4ea9dad267336367c965fe24ae50634b3fa774e74cbd914e29bc74db6a4ac
- id: issue-896-design
  resource: ../../superpowers/specs/2026-10-15-issue-896-typed-session-design.md
  revision: e7e3cae8c4e048ee77d2bf644b47da1792211cbe
  working_tree: clean
  sha256: 889039df4653837fba4f306bc7bf5ba3b24e1d2a54425d8682c6b92a66502644
- id: issue-896-review
  resource: ../../superpowers/reports/2026-10-15-issue-896-review.md
  revision: e7e3cae8c4e048ee77d2bf644b47da1792211cbe
  working_tree: modified
  sha256: 01f6c1cb4bbd07a56eba9df40235dbb6df9abe179fa0f33b4716bdf22c6d7316
- id: session-load-outcome
  resource: ../../../crates/hoimin-core/src/event.rs
  revision: 056cb558953ddf5eb8b229976ee894ae7c6bc87f
  working_tree: modified
  sha256: bca9bd30a23da838ee97a3bcd4ad11933e738ac5c3a07afd298c185823daebe2
- id: current-initialization
  resource: ../../../formal/HoiminOracle/HoiminOracle/SchemaMigrationModel.lean
  revision: 97c394760bc02a56be0bfdeb6301257eac80d1c7
  working_tree: modified
  sha256: 2d1710df34ea77b0733ca1fbb5b10bb9d74d93603959cc0e69a24f04decd91f3
- id: tracing-readme
  resource: ../../usage.md
  revision: e7e3cae8c4e048ee77d2bf644b47da1792211cbe
  working_tree: modified
  sha256: b399ad551c4e410c3cbae8f4a03def0422847e89b7f2d839d573a2129e17e10b
- id: tracing-runtime
  resource: ../../../crates/hoimin-cli/src/telemetry.rs
  revision: 3912d73220917308ce6a966ad2b57ec9b566490c
  working_tree: untracked
  sha256: 394031b25745c4c532e6dabbf6f33eb6a7487d3a952593e00861cce25544abf6
- id: resume-copy-audit
  resource: ../../audits/2026-09-15-resume-copy/README.md
  working_tree: untracked
  sha256: 85e48710c73efb62a5ce970061dc6af55720579ee837a7c4e6fe762acfb61048
- id: issue-464
  resource: ../../superpowers/specs/2026-09-14-issue-464-baseline-output-design.md
  working_tree: untracked
  sha256: 4a10160c54d1b56ab624fc7a28885247bb7e23de622250d3f0ed56a774c7d445
- id: issue-516
  resource: ../../superpowers/specs/2026-09-12-issue-516-session-ownership-identities-design.md
  working_tree: untracked
  sha256: 41e0f7660f2903f726d4c6fe1f9ee974d525864bcf2b8cb90e475e428d64555c
- id: issue-477
  resource: ../../superpowers/specs/2026-09-11-issue-477-import-roots-design.md
  working_tree: untracked
  sha256: a30ed5f129c635e75431212d590171ab03e5973640a4d6624775ee43fd81dcdd

- id: issue-472
  resource: ../../superpowers/specs/2026-09-11-issue-472-session-artifacts-design.md
  working_tree: untracked
  sha256: 5b5845127b78ddbd381fb54394645b515b2c4faf23d996aa05167a2376052724

- id: issue-460
  resource: ../../superpowers/specs/2026-09-11-issue-460-progress-result-design.md
  working_tree: untracked
  sha256: afce723f905aee27b86620e9ac5e81d5db2f408c4f4407e9e1a878b3932c3d33
- id: ownership
  resource: ../../superpowers/specs/2026-09-07-issue-363-session-ownership-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: session
  resource: ../../../crates/hoimin-cli/src/session/mod.rs
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: b58af93b83f10a4617c60327ea98c74b35a5841dd70aa357b7c3ef55df637ecd
- id: ownership-report
  resource: ../../superpowers/reports/2026-09-07-issue-363-session-ownership.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: migration
  resource: ../../superpowers/reports/2026-08-11-lean-schema-migration-concurrency-audit.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: reader
  resource: ../../../crates/hoimin-cli/src/artifacts/run_report.rs
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: c6d0546d6e35310263c0af07e69ec7a2a96b5cd4ff21063ad59265c2ee562d90
- id: schema
  resource: ../../json-schema/run-result.schema.json
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: 52016f274d790ac4aca7a220a2214d79bb31ab20a3815035b7105e63719584a7
- id: issue-488
  resource: ../../superpowers/specs/2026-09-11-issue-488-windows-resource-scope-design.md
  working_tree: untracked
  sha256: 9187c4029df0e3f1e0be45f43d11fa4b875201fcbcb7e0a22a4b4c0f31510d4a

- id: issue-487
  resource: ../../superpowers/specs/2026-09-11-issue-487-resource-report-policy-design.md
  working_tree: untracked
  sha256: 81204e5418b7fe3eac61795402199e9427a1b3157f89525d8267cb502d324e80

- id: issue-484
  resource: ../../superpowers/specs/2026-09-11-issue-484-metrics-destinations-design.md
  working_tree: untracked
  sha256: b146daf296d69374ad01ac86d5a5becd30b721b1983d5a54507cd0f1750ef3e7
- id: integration-484-472
  resource: ../../superpowers/plans/2026-09-11-issue-484-metrics-destinations.md
  revision: 6f6cc91a2a190a4fba8d94a3b1e66caa1f761d5d
  working_tree: modified
  sha256: 5a4e7f914fc2bb31c1a038a63735aa91405fa10a0a531f7cd39189a07b26f9f2
- id: issue-563-repair
  resource: ../../superpowers/specs/2026-09-24-issue-563-design.md
  revision: 51b4a4b61f9e9e10a621eb3574e5caad28e8111c
  working_tree: clean

- id: issue-887-environment
  resource: ../../superpowers/plans/2026-10-14-issue-887.md

- id: issue-895-design
  resource: ../../superpowers/reports/issue-895/design.md
  revision: 62cee6bbd684d13dd69a67bd3a627901103f8624
  working_tree: clean
  sha256: 283d6d959f953906bb88271e7146d18ecc9a39a7ca12813f13552247a30f544d
- id: issue-895-review
  resource: ../../superpowers/reports/issue-895/review.md
  revision: 48d7f94968289be7a8469fd06f42d0702fc9ffba
  working_tree: modified
  sha256: e40737eaa4b50c6cfaa94d52f2859040c7f69c1813bb011f12f6493fbf1fb946
- id: issue-895-outcome
  resource: ../../../crates/hoimin-core/src/resume.rs
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: 657834488419381773ecc3b2c4da69574ad00328723db2f35e729e97b2d6e92e
- id: issue-895-session-schema
  resource: ../../../crates/hoimin-cli/src/session/schema.rs
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: fc255db429a29687c107b9d9eb1acaaa87b97ac0bd1aad84de000cb5891ae38f

- id: issue-949-reader
  resource: ../../../crates/hoimin-cli/src/artifacts/run_report.rs
  revision: ddb8ed9504beb04c06bfe11821ebcafffa332714
  working_tree: modified
  sha256: 585817301a29f83d660b42c293025105adcdc1defd568e1b48a9027f8da6f17a
- id: issue-949-record
  resource: ../../maintenance/2026-10-16-issue-949.md
  revision: 35c21145ccdfb6a044f9ae1dad2a5e4ff90deece
  working_tree: modified
  sha256: bca74014f517c33728e844b9e876a77b24ef3025c7e42fc75afb89d5849580f1
- id: issue-979-compatibility
  resource: ../../compatibility.md
  revision: 4ebed4457519920e9a28f8f653d8ce96985acdad
  working_tree: clean
  sha256: 3db98744cba6c95b2828d98298038c52242135a2607f2855c58d100205243fb4

- id: issue-973-evidence
  resource: ../../maintenance/2026-10-18-issues-971-979/task-4-report.md
  working_tree: untracked
  sha256: 7734e75cb40e9041460b3c816ebf2b8f5b56d72f4a63e8db5ee8f7d0cf74236f
- id: issue-985-admission
  resource: ../../maintenance/2026-10-18-issue-985.md
  revision: c293cf62325f2e39e07760770adeba4a592bea9d
  working_tree: clean
  sha256: d9584be78df3b4d8810d1c52e6188d09cabfbc6156769d7c85b7cf8049a5f305

- id: issue-984-compatibility
  resource: ../../compatibility.md
  revision: c4078e0dadc58bdbc7953b6d5a23cd76160d6ecf
  working_tree: modified
  sha256: 881f774b6a135f3d50f3acca3e379a0c8f0742349fd84964d526caa4073c421a

- id: issue-984-evidence
  resource: ../../maintenance/2026-10-19-issue-984-baseline-restoration.md
  revision: d0d2472ed6797fcc07b286af3056e19dccbedc98
  working_tree: clean
  sha256: 7c4907d6190fa9019c36eb65207a3b5685ebc6da4737612022cb9aead5739ee6

- id: priority-six-compatibility
  resource: ../../compatibility.md
  revision: 3aaf82b0d5b695a548dda33ea248ceaa1dd68219
  working_tree: modified
  sha256: 6dc4ae3fda223f4b3a2a0654042d98eac38f1fe010e267f79321d282388f0eb2

---

# 導出したstatusの送受信（Issue #1010）

report schema 9では、JSONの`mutants[]`とJSONLの`mutant_finished`に必須の`status`を出力する。reportの直列化処理は`MutationOutcome::status()`から値を計算する。読取り処理は受け取った値とoutcomeの分類を照合し、矛盾を拒否した後にstatusを破棄する。実行状態、集計、progress、resumeの分類根拠はoutcomeのままである。`Exit(1)`でも出力回収が`close_timed_out`なら`error`となる。[^issue-1010-wire]

JSON Schemaはstatusの必須性と七つの値を検査する。outcomeとの意味上の一致はRustのstrict readerが検査するため、汎用のschema検証だけでは一致を確認できない。旧schemaの番号だけを書き換えても現行reportにはならない。[^issue-1010-wire]

humanの完了行には、候補ID、位置、演算子、引用符付きの元の式と置換後の式、判定をまとめる。共通の`HumanText`で各外部文字列を一度escapeするため、制御文字を含む場合も完了行は一行を保つ。並列実行の完了順が開始順と異なっても、候補IDを開始行で探す必要はない。[^issue-1010-human]

# Current report identity validation (Issue #992)

The common current JSON/JSONL reader checks each candidate's saved ID against
`CandidateIdentityRef` after structural validation. Progress and review consume
this guarantee and reject mismatches with exit 2. Progress requires no source
workspace access; review retains its additional checked-plan and source checks.
The identity uses schema, source hash, canonical path, span, operator and
replacement, excluding display line, column, symbol and discovery order.
Both borrowed and owned projections use one hash encoder, preserving mutant-ID
v1 bytes and valid schema-8 writer output.[^issue-992-identity][^issue-992-reader]

# Single project root in saved reports (Issue #949)

Issue #949 introduced the single root in report schema 7. Current schema 9
stores it only in
`run.normalized_config.selection.root` for JSON and
`normalized_config.selection.root` for `run_started` JSONL. Typed configuration
readers reject the former outer root even when its value agrees. Review resolves
its project identity from the one selection root. Progress retains its opaque
configuration contract and still checks report versions and event structure.
Older report versions require their producing binary or a rerun; changing a
schema number cannot convert old reports.[^issue-949-reader][^issue-949-record]

That change raised the floor to 0.11.0 and retained fingerprint 11 and session 5.
Issue #979 introduced floor 0.12.0, report/event schema 8 and fingerprint 12;
runtime configuration and execution fingerprints omit discovery timeout.
Issue #984 raised the floor to 0.13.0 and fingerprint to 13 so incomplete
sessions cannot reuse conclusive results produced before baseline restoration.
Session schema 5 remains. Different ample plan timeouts produce the same current
execution fingerprint when discovery results agree; historical schema-11 and
schema-12 fingerprint bytes are incompatible. The environment field-13 framing
remains unchanged. The combined schema-9 cleanup/status change raises the package
floor to 0.14.0 while retaining fingerprint 13. Earlier sections retain their
historical versions.[^issue-949-record][^issue-979-compatibility][^issue-984-compatibility][^priority-six-compatibility]

A new successful baseline and restoration do not recompute a reused `killed` or
`survived` result. The fingerprint version therefore owns this execution-policy
boundary. When no compatible current incomplete run exists, an older latest
incomplete fingerprint can produce `session.resume.incompatible`. Users can keep
the DB and choose a new path or a deliberate non-resume run. Compatible current
history still wins, and completed old history retains fresh-run behavior; no
conversion or deletion is performed.[^issue-984-compatibility]

The historical-session regression uses genuine frozen schema-12 input/digest
and a stored executed failure. Reverting only the epoch makes that fixture
incorrectly resumable; restoring schema 13 rejects it and preserves its rows.
Current reuse, completed-old fresh behavior and current-compatible preference
remain positive controls. This is a typed SQLite fixture and executed handler
check, not a rebuilt historical binary.[^issue-984-evidence]

[^issue-984-compatibility]: [Baseline restoration and result reuse](../../compatibility.md#baseline-restoration-and-result-reuse-in-0130).
[^issue-984-evidence]: [Resume compatibility checks and limits](../../maintenance/2026-10-19-issue-984-baseline-restoration.md).

[^issue-949-reader]: [Shared report readers](../../../crates/hoimin-cli/src/artifacts/run_report.rs).
[^issue-949-record]: [Issue #949 design, self-reviews and verification](../../maintenance/2026-10-16-issue-949.md).


# Session load result

SessionLoaded carries exactly one SessionLoadOutcome: Resumed owns the saved run
reference, and Fresh carries one observed history reason. The machine selects its
next effects with one match. Missing outcomes and legacy resume/fresh_reason fields
are rejected; no fallback supplies a reason. Report ResumeOutcome remains a separate
status/reason contract. The public no_compatible_run reason is removed at floor
0.7.0; report schema 5 includes the environment snapshot change, while persisted
session rows do not change.[^session-load-outcome][^issue-887-environment]


# 実行単位ごとの所有権

sessionは、一回の実行（run）の結果を保存・再利用する仕組みである。保存処理を担当する `SessionHandler` は、`begin` または成功した `load` で、そのrunを操作する所有権を得る。[^ownership]

結果保存の `persist` と再開時の結果検索 `lookup` には、要求されたrunの所有権が必要である。他のrunを所有していても代用できない。`finish` で所有権を解放した後は、未完了のrunでも再取得が必要になる。現在の実装でも、DB処理より前に `require_ownership` を呼ぶことを静的に確認した。[^ownership][^session]

所有権がなければ、保存は `session.persist.owner`、検索は `session.lookup.owner` で先に拒否する。そのため、破損DBやトランザクション失敗を調べるテストデータ（fixture）は、先に正当な所有権を確立してから障害を起こす必要がある。所有権がないままでは、試験したいDB処理へ到達しない。[^ownership-report]

Issue484の設計では、metrics出力によるsession DB、SQLite副ファイル、所有権ロックの置換も衝突検査の対象とする。WALによる復旧を上書きの安全性の根拠にせず、使用中の具体的なパスを保護する。別のリンクエントリを置き換える場合と、DBやロック自体のエントリを置き換える場合を区別する。[^issue-484]

保存先の同一性を確定できない場合は、実行結果を維持してmetricsの保存を見送り、終了時に `metrics.write` で通知する。未作成の親ディレクトリや対応範囲外の別名もこの扱いとし、既知の衝突をbaseline前に拒否する場合と区別する。[^issue-484]

# 実行中のsession生成物とworkspace検査

Issue #472 の設計では、指定されたsessionのDB、WAL・SHM・journal、所有権ロック用ディレクトリをpreflightのsnapshot作成前に特定し、workerコピーと元workspaceの変更検査から共通の規則で除外する。除外対象は実際に使うパスで指定し、通常のDB fixtureや似た名前のファイルには広げない。明示的なincludeでも実行中のsession生成物をコピー対象に戻さない。[^issue-472]

相対sessionパスは呼出し元のカレントディレクトリを基準に解決する。SQLiteを開くパスとロック名の決定に同じ解決結果を使い、DBを早期作成してsnapshotへ紛れ込ませない。所有権の取得と排他制御は維持し、元ソースや通常fixtureの変更は従来どおり拒否する。これは選択規則の設計であり、過去のLean監査結果を新しい実装全体の保証として扱わない。[^issue-472]

# session初期化と過去の移行監査

2026-08-11のスキーマ移行監査では、DBの書込みロック取得後の版再確認、失敗時に変更をまとめて取り消す処理、旧データの保持、未対応の新しい版の扱いをモデル化した。公開APIで二つのopenを同時に始める試験では、両者が古い版を読んでからロックを取ったかまでは確定できない。この特定の実行順序を要求するケースは、モデル内だけで確認する `model-only` に分類している。[^migration]

一方、run所有権の修正報告では、macOSとLinuxの非rootユーザー環境で、Leanの期待値と公開APIの結果を比較する20件の `strict` ケースが一致した。有限の実行順序を調べる探索は深さ4までであり、深さ8は当時の時間制限で完了していない。これらの結果が示す範囲に、Windows・実際のクラッシュ・SQLite自体の正しさは含まれない。[^ownership-report]

Issue #895以降、session DBはschema 5だけを開く。
新規DB（`user_version = 0`）にはschema 5を一つのトランザクションで直接作成し、旧版1〜4と未対応の新しい版は変更せず拒否する。
過去の移行監査は当時の実装の記録であり、現在の移行対応を示さない。
`SchemaMigration`という既存のモジュール名を保つ現行モデルとcorpusは、新規初期化の競合、ロック取得後の版再確認、初期化失敗のrollback、未対応版の拒否を対象とする。
SQLiteのWAL実装や実際のクラッシュを証明するものではない。[^current-initialization]

# 結果の生成と読取りの責務

JSON Schemaは結果の構造を定める元資料として保持する。ただし、構造上正しいJSONにも、値どうしが矛盾した結果は入り得るため、読取り処理で意味上の整合性も検査する。現在のprogress読取り処理は、件数検査後に `validate_summary_coherence` を呼び、完了状態と終了コードが整合するか確認する。[^schema][^reader]

この集計値の検査の詳細は[progress入力の後続修正](../audits/progress-input.md)を参照する。Issue #460 の設計は、個々の結果の検証を `MutantFinished::validate_result` にまとめ、生成側の `ReportSequence` とprogress読取りで共有する。終了理由がある場合は出力状態も含めてstatusを照合し、矛盾した入力は比較処理へ渡す前に拒否する。この旧検証はIssue #895のtyped outcomeへ置き換わり、独立した可変statusやnullable実行証拠は受理しない。schema 9のstatusは、outcomeと照合して破棄する表示値である。[^issue-460]

# progressのレポート入力スキーマ

progressは `REPORT_SCHEMA_VERSION`（6）に一致するJSON文書とJSONLストリームを読む。旧schema 2/3/4/5と未対応の将来版を拒否し、旧形式専用の型・変換・検証は持たない。reviewは現行スキーマのJSONを要求する。[^reader]

スキーマ限定後も、候補ID・run ID・sequence・summary・単一結果の検証を維持する。JSONLの一行ずつの読取り、設定を不透明なJSONとして扱うprogressの方針も維持する。旧レポートは生成元バイナリで読むか、意図する版で実行し直す。planの再生成だけでは過去のreportは復元されない。手順と互換性通知は[利用方法](../../usage.md)と[互換性方針](../../compatibility.md)を参照する。[^reader]

# 再確認条件

run所有権、再利用条件、結果置換、DBスキーマ、集計値と終了コードの規則、JSON読取り処理を変更したら更新する。保存・読取り・比較の各処理で同じ前提が維持されるかを調べる。旧報告にある未解決という記述は、その報告の時点として扱う。

# Windowsの制限値を読むときの単位

Issue488では、Windowsのnormalized_config内のmax-memory／max-processesを、各root process treeの上限として説明する。resource_modeのhardはOSによる強制方式を表し、複数のtreeを合計した上限を意味しない。jobsを増やした場合の合計量も考慮して設定する必要がある。[^issue-488]

この制限単位の修正は、開始・再利用結果へbackend情報を渡すIssue487とは独立している。既存の出力構造を説明するためだけに重複した情報伝播APIや新たなschema欄を追加しない。[^issue-488]

# 選択した資源制限方式と再利用結果

Issue487では、実際に選択したbackendのmodeと識別用mechanismを、run開始前にcoreへ渡す。runヘッダーと、再利用・停止によって生成する結果は同じrunの方針を表す。best-effortを許可する設定から実行方式を推測せず、実プロセスの結果は実行側から受け取ったmodeを保つ。[^issue-487]

再利用結果のterminationとoutputはnull、経過時間は0のままとする。過去のプロセスを今回実行したことにはせず、保存済みの履歴も表示のために書き換えない。JSONとJSONLの実行、自然に未完了となったsessionからのresumeを通じて、ヘッダー・実行結果・再利用結果を照合する。[^issue-487]

# Issue 477: import順序と再利用条件

明示import rootの順序は実行されるモジュールを変え得るため、正規化した順序付きリストをfingerprintへ加える。fingerprint schemaはv6からv7へ進め、旧sessionは既存の版不一致診断に従う。rootの値や順序が異なる結果を再利用せず、同じ順序の重複は除く。過去のrunレポートでフィールドがない場合の読取りと、旧plan・旧sessionの実行互換性は別に扱う。[^issue-477]

[^ownership]: [2026-09-07-issue-363-session-ownership-design.md](../../superpowers/specs/2026-09-07-issue-363-session-ownership-design.md)。
[^session]: [mod.rs](../../../crates/hoimin-cli/src/session/mod.rs)。
[^ownership-report]: [2026-09-07-issue-363-session-ownership.md](../../superpowers/reports/2026-09-07-issue-363-session-ownership.md)。
[^migration]: [2026-08-11-lean-schema-migration-concurrency-audit.md](../../superpowers/reports/2026-08-11-lean-schema-migration-concurrency-audit.md)。
[^reader]: [run_report.rs](../../../crates/hoimin-cli/src/artifacts/run_report.rs)。
[^schema]: [run-result.schema.json](../../json-schema/run-result.schema.json)。

[^issue-488]: [2026-09-11-issue-488-windows-resource-scope-design.md](../../superpowers/specs/2026-09-11-issue-488-windows-resource-scope-design.md)。

[^issue-487]: [2026-09-11-issue-487-resource-report-policy-design.md](../../superpowers/specs/2026-09-11-issue-487-resource-report-policy-design.md)。

[^issue-484]: [2026-09-11-issue-484-metrics-destinations-design.md](../../superpowers/specs/2026-09-11-issue-484-metrics-destinations-design.md)。

[^issue-477]: [2026-09-11-issue-477-import-roots-design.md](../../superpowers/specs/2026-09-11-issue-477-import-roots-design.md)。

[^issue-472]: [2026-09-11-issue-472-session-artifacts-design.md](../../superpowers/specs/2026-09-11-issue-472-session-artifacts-design.md)。

[^issue-460]: [2026-09-11-issue-460-progress-result-design.md](../../superpowers/specs/2026-09-11-issue-460-progress-result-design.md)。

# sessionとmetricsの統合後のパス保護

Issue472と484の統合後は、metricsもSessionHandlerと同じSessionArtifactsから実DB・副ファイル・所有権ロックのパスを得る。正規化前の設定パスを検査し、symlink自体と正規化された実DBの両方を保護する。Windowsでは設定名側の副ファイルも保護対象として維持するため、実DBと別名であってもそれらをmetrics出力先には使えない。[^integration-484-472]

検査後にsessionのコピー除外と正規化を行い、metrics出力の許可結果と実DBのパスを同じ完了通知で引き渡す。新しいWindows専用検証では、symlink作成の省略を成功として扱わない。実機結果は統合記録で別途確認する。[^integration-484-472]

[^integration-484-472]: [Issue484の実装・統合記録](../../superpowers/plans/2026-09-11-issue-484-metrics-destinations.md)。

# active session の所有権ディレクトリ（Issue #516）

session の所有権ロックを置くディレクトリについて、指定パスから構成した表記と、既存ディレクトリを canonicalize した実体パスを `SessionArtifacts` から取得する。workspace の事前除外と metrics の衝突判定が同じ一覧を使う。これにより、ロック用の別名がプロジェクト内の実体ディレクトリを指す場合も、hoimin 自身の書込みを元ソースの変更と誤認しないようにする。[^issue-516]

macOSで、並列数1・2、root内外の別名、大小文字の別名、実際のタイムアウト後の再開を検証した。Linux・Windowsの実機検証はローカルでは行っていない。既存の I/O 事前処理でのみ実体を解決し、存在しないロックディレクトリを作成しない。元ソース・通常の fixture の変更検知、所有権競合の検出、metrics の安全な別エントリ置換は維持する。[^issue-516]

[^issue-516]: [Issue 516: Share active session ownership-tree identities](../../superpowers/specs/2026-09-12-issue-516-session-ownership-identities-design.md)。

## Environment metadata and report readers (Issue #887)

Issue #1004 adds semantic acceptance to strict review's typed `RunConfig`
deserializer. Invalid selectors, operators, argv, limits, and non-normalized
import roots fail before project-root canonicalization. The deserializer reads
the original configuration object before checking it, preserving duplicate-key
rejection. Progress still selects an opaque configuration type and accepts an
object or null without these semantic or filesystem checks. The saved report
shape remains unchanged.[^issue-1004-reader][^issue-1004-config]

[^issue-1004-reader]: [Shared reader and regressions](../../../crates/hoimin-cli/src/artifacts/run_report.rs).
[^issue-1004-config]: [Checked configuration boundary](../../../crates/hoimin-core/src/config.rs).

Report schema 6 retains the validated `environment` snapshot introduced in schema
5 in place of separate names and an optional hash. Progress and review consume
the writer's schema; preserve schema-5 and older reports with their producing
binary or rerun rather than rewriting their version fields. Captured plaintext never enters snapshot metadata.
Plan schema 7 requires regeneration of older plans. Fingerprint schema 11 and its
exact bytes remain stable, so this format change alone does not require a new
session; the current typed-outcome contract uses SQLite schema 5. [^issue-887-environment]

[^issue-887-environment]: [Issue 887 typed snapshot verification](../../superpowers/plans/2026-10-14-issue-887.md).

## baseline失敗時の保存済み出力

`run` と `verify` は失敗またはプロセスの時間切れとなったbaselineについて、cleanup前に保存済みの標準出力・標準エラーをstderr診断へ逐次出力する。`--max-output` の末尾保持上限に従い、保持量・観測量・切詰め有無とraw byte offsetを示す。非UTF8は置換文字、端末制御文字はエスケープで表す。stdoutのスキーマは変えない。保存にはstderrのリダイレクトを使う。キャンセルや出力先の詰まりで総時間制限に達すると出力は途中で止まりうる。[^issue-464]

[^issue-464]: [原文](../../superpowers/specs/2026-09-14-issue-464-baseline-output-design.md)。

## コピー方針変更時の再開互換性

`5e631ef`ではinclude/excludeがfingerprintへ含まれず、コピーされる補助ファイルが変わっても旧条件のkilled/survivedを再利用した（#563）。[監査の証拠と限界](../audits/resume-copy-2026-09.md)に、設定変更の4反例と据置き・出力上限変更の正例を記録した。修正時にはコピー設定の比較方法とfingerprint schemaの更新を決める。[^resume-copy-audit]

[^resume-copy-audit]: [コピー方針とsession再開](../../audits/2026-09-15-resume-copy/README.md)。

## 2026-09-24: Issue #563の修正

2026-09-24の#563修正では、include/excludeの順序付きリストをfingerprintへ追加し、schemaを8へ進めた。パターンの表記と順序を保持するため、同じ意味でも異なる表記の設定では新規runになる場合がある。旧fingerprintの結果を新しいrunへ混在させず、jobs/max-outputだけの変更は従来どおり互換とする。[^issue-563-repair]

[^issue-563-repair]: [修正設計](../../superpowers/specs/2026-09-24-issue-563-design.md)。


## 実行中の進捗と診断ログ（2026-09-26）

`run`・`plan`・`verify` は、標準エラー出力（stderr）が端末なら、処理段階・完了件数・経過時間を約1秒ごとに表示する。完了件数には保存済み結果の再利用を含め、`not_run` を除く。候補を作るだけの `plan` では完了件数は0となる。stderrをリダイレクトすると進捗表示は止まり、stdoutには選択した形式の結果を出力する。保存済みレポートを比較する `progress` コマンドは、この実行中表示の対象に含めない。[^tracing-readme]

診断ログは `RUST_LOG=hoimin_cli=debug` などで有効にする。端末stderrではテキスト、リダイレクト先ではJSON Linesを使う。有効時には既存の診断イベントとtracingのログが同じstderrに出るため、読取り側は両形式を区別する必要がある。tracingのログにはrun-eventのスキーマを適用しない。未指定・空文字・無効なフィルター・`off` ではログを出さず、端末の進捗表示は続ける。[^tracing-readme][^tracing-runtime]

CLIはログを容量制限付きキューから別スレッドへ渡す。キューが満杯の場合や1レコードが64 KiBを超える場合は、レコード全体を破棄する。終了時の出力待ちは最大250 msとするため、診断ログの完全な保存は保証しない。変異結果の集計にはレポートを使う。ライブラリを組み込む場合は、呼出し側でtracing subscriberを設定する。[^tracing-runtime]

端末判定、フィルター、完了件数、stderrの書込み単位を変える場合は、通常実行・再開・出力先の詰まりを再検証する。今回の実行結果とOSの範囲は[レビュー記録](../../superpowers/reports/2026-09-26-tracing-progress.md)を参照する。

[^tracing-readme]: [利用方法](../../usage.md)。
[^tracing-runtime]: [CLIの診断出力](../../../crates/hoimin-cli/src/telemetry.rs)。

[^current-initialization]: [現行session初期化モデル](../../../formal/HoiminOracle/HoiminOracle/SchemaMigrationModel.lean)。

[^session-load-outcome]: [Session completion contract](../../../crates/hoimin-core/src/event.rs).

## Typed mutation outcomes (Issue 895)

`MutantResult` and report-schema-6 `MutantFinished` own one `MutationOutcome`.
Executed outcomes require termination, explicit output state, output spool, and
elapsed duration. Status derives centrally from this evidence, including the
output-close-timeout error override. Reused outcomes permit only killed or
survived; `NotRun` means no classified completed result is available, even when
a process started. Synthetic outcomes project zero elapsed and absent output.
Diagnostics remain independent evidence and never reconstruct output state.
Baseline execution policy and report fields are unchanged. [^issue-895-design][^issue-895-outcome]

Session schema 5 stores outcome JSON directly and derives the narrow StoredResult
status on lookup. Old flat result wire and initialized session versions 1-4 are
rejected without adapters. [^issue-895-session-schema] New current report/session goldens retain historical
files separately. Report lifecycle sequence, identity, and summary coherence
remain independently validated. The typed report-sequence Lean model retains
lifecycle proofs and explicit synthetic/output-close cases; those claims do not
prove operating-system execution or cancellation behavior. [^issue-895-review]

Recheck execution classification, reuse and stopped-drain semantics, strict JSON
payloads, transaction replacement and diagnostic preservation, schema fixtures,
and both report/progress oracle projections when changing this boundary.

[^issue-895-design]: [Typed mutation outcome design](../../superpowers/reports/issue-895/design.md).
[^issue-895-review]: [Issue 895 review and verification ledger](../../superpowers/reports/issue-895/review.md).

[^issue-895-outcome]: [MutationOutcome and result domain](../../../crates/hoimin-core/src/resume.rs).
[^issue-895-session-schema]: [Current session schema](../../../crates/hoimin-cli/src/session/schema.rs).


## Typed changed selection (Issue #899)

Plan schema 7 and report schema 6 store changed selection as null when disabled,
or as an object containing required bounded `context` and optional `diff_base`
when enabled. Boolean changed values and the former sibling context/base fields
are rejected. Regenerate older plans; preserve older reports with their producing
binary or rerun. No conversion updates existing files. The CLI's flags, explicit
selector intersections, Git base semantics, and effective target fingerprints
retain their meaning. [^issue-899-changed-selection][^issue-899-changed-review]

[^issue-899-changed-selection]: [Typed changed selection design](../../superpowers/specs/2026-10-15-issue-899-design.md).

[^issue-899-changed-review]: [Issue 899 review and verification](../../superpowers/reports/2026-10-15-issue-899-review.md). Current executed scopes, fixture corrections, limitations and release-floor checks.

## Normalized session intent (Issue #896)

Raw CLI input retains `--session` and `--resume`. Normalization converts them once
into `SessionMode::Disabled`, `Fresh(SessionConfig)` or `Resume(SessionConfig)`.
An enabled mode owns its path; a resumed mode without a session path cannot be
constructed. Path normalization changes the configuration inside the enabled
mode without replacing its intent.[^issue-896-config][^issue-896-design]

Report schema 6 serializes disabled intent as `{"mode":"disabled"}` and enabled
intent as `{"mode":"fresh","config":{"path":"..."}}` or the corresponding
`resume` mode. Normalized readers reject the obsolete independent `resume` field
and old session objects. Plan schema 7 still omits runtime session intent, and
session database schema 5 stores typed mutation outcomes. `SessionLoadOutcome` and report
`ResumeOutcome` describe the observed selection result, not the request mode.
CLI options, session exclusions and metrics collision rules remain in their
existing guides.[^issue-896-config][^issue-896-design]

The branch review distinguishes static inspection, Windows checks and pending
platform checks. Changing session normalization or path ownership requires
rechecking all three modes, strict Schema decoding and collision rules.[^issue-896-review]

[^issue-896-config]: [Normalized configuration](../../../crates/hoimin-core/src/config.rs).
[^issue-896-design]: [Typed session design](../../superpowers/specs/2026-10-15-issue-896-typed-session-design.md).
[^issue-896-review]: [Review and verification](../../superpowers/reports/2026-10-15-issue-896-review.md).

## Required persisted budgets (Issue #916)

New schema-5 session databases require a positive eight-byte BLOB budget with
NOT NULL enforced independently of CHECK constraints. Existing schema-5 databases
with nullable DDL remain readable and writable without schema rewrites when
their budget values satisfy that contract. NULL is corrupt stored data and
returns `session.corrupt`, as do other invalid storage types; it no longer has a
separate compatibility diagnosis. Selection, ownership rechecks and conditional
updates retain their ordering. Schema version 5, release floor 0.8.0 and unsigned
big-endian budget encoding remain unchanged.[^issue-916-budget]

The original schema-5 golden retains the previous nullable DDL for compatibility
and corrupt-reading tests. A separate required-budget golden checks complete
schema and logical-row regeneration for newly created databases. Budget/resume
oracle correspondence concerns current positive budgets and eligibility; old
NULL migration conventions are historical, not supported reader states.

[^issue-916-budget]: [Required session budget design and validation plan](../../superpowers/plans/2026-10-15-issue-916.md).

# Required saved run arrays (Issue #923)

`RunConfig` requires an explicit `import_roots` array, as it already requires
`fingerprint_files`. Missing and null arrays fail typed configuration/event
reading and review's report reader. Empty arrays remain valid. Progress keeps
`normalized_config` opaque and does not apply these typed field requirements.
Report schema 6, session schema 5 and fingerprint bytes remain unchanged. Read
affected reports with their producing binary or rerun; current writer output
needs no regeneration.[^issue-923-config][^issue-923-compatibility]

[^issue-923-config]: [Normalized configuration types](../../../crates/hoimin-core/src/config.rs).
[^issue-923-compatibility]: [Required saved arrays compatibility notice](../../compatibility.md#required-saved-configuration-arrays-in-090).

## Typed reuse decision (Issue 940)

`resume_policy` returns `ResumeDecision::Reuse(ReusedMutationStatus)` for Killed
and Survived. Its callers receive the reusable status from the decision, without
reading the broad stored status again. StoredResult still permits Timeout,
OutOfMemory, ProcessLimit, Error and NotRun; those results require rerunning.
[^issue-940-policy]

The public Rust enum now carries a payload. Its derived Serde reuse value changes
from `"Reuse"` to an object such as `{"Reuse":"killed"}`. Callers must bind the
payload or match `Reuse(_)`; there is no compatibility adapter. Report schema 6,
session schema 5 and their result formats remain unchanged. Review records the
repository consumers and checks; changing reuse eligibility requires checking
both policy and persisted-result round trips.[^issue-940-review]

[^issue-940-policy]: [Resume decision and policy](../../../crates/hoimin-core/src/resume.rs).
[^issue-940-review]: [Issue 940 review and verification](../../maintenance/2026-10-16-issue-940.md).

## Duplicate report configuration fields (Issue #939)

Review decodes `run.normalized_config` into `RunConfig` as part of the original
JSON document decode. It rejects duplicate known configuration fields before a
map can discard them, including nested fields and equivalent escaped key names.
Required-field, null, unknown-field and typed-value checks remain in the existing
configuration decoders. The private report document selects its configuration
type per consumer; schema and event structure validation stay shared.[^issue-939-reader]

Progress selects an opaque JSON value and checks only that configuration is an
object or null. Both JSON and JSONL retain usable/unusable classification without
review's typed configuration or filesystem checks. Report schema 6 and valid
writer output remain unchanged; duplicate rejection corrects invalid-input
acceptance and requires no regeneration or version-floor increase.[^issue-939-reader][^issue-939-compatibility]

The verification record distinguishes raw-JSON RED/GREEN regressions, preserved
progress behavior, existing Lean corpus correspondence and broader test results.
Recheck these contracts when changing the report header representation,
configuration decoding or the JSON/JSONL read boundary.[^issue-939-verification]

[^issue-939-reader]: [Shared report reader](../../../crates/hoimin-cli/src/artifacts/run_report.rs).
[^issue-939-compatibility]: [Duplicate report configuration compatibility notice](../../compatibility.md#duplicate-report-configuration-fields).
[^issue-939-verification]: [Issue 939 implementation and verification](../../maintenance/2026-10-16-issue-939.md).

# Preflight fingerprint ownership (Issue #973)

`RunState::with_fingerprint` was removed because preflight overwrote its value.
Fresh BeginSession, resumed LoadSession and a fresh fallback after LoadSession
all use the preflight fingerprint. Enabled sessions without it still fail with
`MissingFingerprint`; disabled sessions need none, and stopping before preflight
does not begin a session. No compatibility constructor remains.[^issue-973-evidence]

[^issue-979-compatibility]: [Current format and fingerprint migration](../../compatibility.md).
[^issue-973-evidence]: [Fingerprint-flow characterization and constructor removal](../../maintenance/2026-10-18-issues-971-979/task-4-report.md).

# Regular-file report inputs (Issue #985)

Progress opens reports through the shared regular-file admission helper and
parses the validated descriptor. On Unix, nonblocking open followed by descriptor
metadata validation rejects writerless FIFOs, including symlink paths, without
waiting for a writer. Regular-file symlinks,
JSON/JSONL parsing and caller-specific errors retain their behavior; review's
64 MiB input cap is not applied to progress. This admission rule does not bound
all filesystem operation times or process RSS.[^issue-985-admission]

The public CLI tests independently exercise the file kinds, both report formats
and an input exceeding review's cap. Their deadline guard kills and reaps the
child on regression. The record reports macOS tests and leaves other platform
execution explicit.[^issue-985-admission]

[^issue-985-admission]: [Regular-file admission and verification record](../../maintenance/2026-10-18-issue-985.md).

[^issue-992-identity]: [crates/hoimin-core/src/candidate.rs](../../../crates/hoimin-core/src/candidate.rs).
[^issue-992-reader]: [crates/hoimin-cli/src/artifacts/run_report.rs](../../../crates/hoimin-cli/src/artifacts/run_report.rs).


# External strings in human reports (Issue #1002)

Human run events, fingerprint provenance, session diagnostics and ordinary event
diagnostics share the `HumanText` presentation policy with preview, review and
progress. External fields escape controls and line separators while preserving
ordinary Unicode. Renderer-owned separators remain actual newlines. Machine
serializers continue to receive the original strings.[^issue-1002-human-text][^issue-1002-human-report]

The `baseline.output` diagnostic body alone uses an explicit multiline projection:
LF/TAB remain readable, other controls are escaped, and existing textual escapes
are preserved once. Its code and level still use ordinary single-line projection.
This preserves the separate child-output streaming contract without changing
capture, chunking or JSON/JSONL messages.[^issue-1002-human-report][^issue-1002-human-tests]

Parameterized renderer tests cover C0/C1 controls, Unicode, literal backslashes,
and baseline layout. Property tests require single-line human fields to contain
no control characters or Unicode line separators. JSON/JSONL tests compare the
original candidate and diagnostic values after parsing. These are native macOS
results, not evidence of native Linux/Windows execution or of Unicode visual
spoofing prevention.[^issue-1002-human-tests]

[^issue-1002-human-text]: [Shared human text projection](../../../crates/hoimin-cli/src/human_text.rs).
[^issue-1002-human-report]: [Human event and diagnostic rendering](../../../crates/hoimin-cli/src/report/human.rs).
[^issue-1002-human-tests]: [Human and machine report regression tests](../../../crates/hoimin-cli/tests/report_handler.rs).

## Janitor report schema 9 (Issue #1015)

Report/event schema 9 replaces `disk.stale_roots_reclaimed` with required nullable
`disk.janitor`. Rust decoding rejects a missing field; explicit null means no
observation. Non-null summaries distinguish inspected, reclaimed, already absent,
preserved and unresolved roots, acknowledged deleted entries, and scan/detail
omissions. Readers reject older report versions instead of adapting their shapes.
[^issue-1015-wire]

The 0.14.0 package floor carries this report change. Plan schema 10, session schema
5, fingerprint 13 and candidate identities do not change. CLI `--verbose` is passed
as runtime ReportPresentation, outside checked/saved RunConfig and PlanConfig.
Default and verbose output share factual counts and exit policy; only ordinary
retention details and their presentation-suppression count differ. This section
supersedes earlier current report-version and floor statements, without changing
historical verification evidence.[^issue-1015-compatibility]

[^issue-1015-wire]: [Core report types and required-nullable decoder](../../../crates/hoimin-core/src/report.rs).
[^issue-1015-compatibility]: [Schema 9 compatibility notice](../../compatibility.md#structured-startup-cleanup-in-0140).

[^issue-1010-wire]: [MutantFinishedの直列化と検査](../../../crates/hoimin-core/src/report.rs)。
[^issue-1010-human]: [humanの完了行](../../../crates/hoimin-cli/src/report/human.rs)。

[^priority-six-compatibility]: [Current combined schema-9 notice](../../compatibility.md#structured-startup-cleanup-in-0140).
