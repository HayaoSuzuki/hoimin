---
type: Contract
title: session所有権とレポートの検証
description: 保存・復旧の権限、スキーマ移行、結果の生成側と読取り側の検証責務を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
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
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: ownership-report
  resource: ../../superpowers/reports/2026-09-07-issue-363-session-ownership.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: migration
  resource: ../../superpowers/reports/2026-08-11-lean-schema-migration-concurrency-audit.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: reader
  resource: ../../../crates/hoimin-cli/src/progress/input.rs
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: schema
  resource: ../../json-schema/run-result.schema.json
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
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

---

# 実行単位ごとの所有権

sessionは、一回の実行（run）の結果を保存・再利用する仕組みである。保存処理を担当する `SessionHandler` は、`begin` または成功した `load` で、そのrunを操作する所有権を得る。[^ownership]

結果保存の `persist` と再開時の結果検索 `lookup` には、要求されたrunの所有権が必要である。他のrunを所有していても代用できない。`finish` で所有権を解放した後は、未完了のrunでも再取得が必要になる。現在の実装でも、DB処理より前に `require_ownership` を呼ぶことを静的に確認した。[^ownership][^session]

所有権がなければ、保存は `session.persist.owner`、検索は `session.lookup.owner` で先に拒否する。そのため、破損DBやトランザクション失敗を調べるテストデータ（fixture）は、先に正当な所有権を確立してから障害を起こす必要がある。所有権がないままでは、試験したいDB処理へ到達しない。[^ownership-report]

Issue484の設計では、metrics出力によるsession DB、SQLite副ファイル、所有権ロックの置換も衝突検査の対象とする。WALによる復旧を上書きの安全性の根拠にせず、使用中の具体的なパスを保護する。別のリンクエントリを置き換える場合と、DBやロック自体のエントリを置き換える場合を区別する。[^issue-484]

保存先の同一性を確定できない場合は、実行結果を維持してmetricsの保存を見送り、終了時に `metrics.write` で通知する。未作成の親ディレクトリや対応範囲外の別名もこの扱いとし、既知の衝突をbaseline前に拒否する場合と区別する。[^issue-484]

# 実行中のsession生成物とworkspace検査

Issue #472 の設計では、指定されたsessionのDB、WAL・SHM・journal、所有権ロック用ディレクトリをpreflightのsnapshot作成前に特定し、workerコピーと元workspaceの変更検査から共通の規則で除外する。除外対象は実際に使うパスで指定し、通常のDB fixtureや似た名前のファイルには広げない。明示的なincludeでも実行中のsession生成物をコピー対象に戻さない。[^issue-472]

相対sessionパスは呼出し元のカレントディレクトリを基準に解決する。SQLiteを開くパスとロック名の決定に同じ解決結果を使い、DBを早期作成してsnapshotへ紛れ込ませない。所有権の取得と排他制御は維持し、元ソースや通常fixtureの変更は従来どおり拒否する。これは選択規則の設計であり、過去のLean監査結果を新しい実装全体の保証として扱わない。[^issue-472]

# スキーマ移行の競合と検証範囲

スキーマ移行の監査では、DBの書込みロック取得後の版再確認、失敗時に変更をまとめて取り消す処理、旧データの保持、未対応の新しい版の扱いをモデル化した。公開APIで二つのopenを同時に始める試験では、両者が古い版を読んでからロックを取ったかまでは確定できない。この特定の実行順序を要求するケースは、モデル内だけで確認する `model-only` に分類している。[^migration]

一方、run所有権の修正報告では、macOSとLinuxの非rootユーザー環境で、Leanの期待値と公開APIの結果を比較する20件の `strict` ケースが一致した。有限の実行順序を調べる探索は深さ4までであり、深さ8は当時の時間制限で完了していない。これらの結果が示す範囲に、Windows・実際のクラッシュ・SQLite自体の正しさは含まれない。[^ownership-report]

# 結果の生成と読取りの責務

JSON Schemaは結果の構造を定める元資料として保持する。ただし、構造上正しいJSONにも、値どうしが矛盾した結果は入り得るため、読取り処理で意味上の整合性も検査する。現在のprogress読取り処理は、件数検査後に `validate_summary_coherence` を呼び、完了状態と終了コードが整合するか確認する。[^schema][^reader]

この集計値の検査の詳細は[progress入力の後続修正](../audits/progress-input.md)を参照する。Issue #460 の設計は、個々の結果の検証を `MutantFinished::validate_result` にまとめ、生成側の `ReportSequence` と、現行・旧形式のprogress読取りで共有する。終了理由がある場合は出力状態も含めてstatusを照合し、矛盾した入力は比較処理へ渡す前に拒否する。旧形式で許容する終了理由のnullは維持する。[^issue-460]

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
[^reader]: [input.rs](../../../crates/hoimin-cli/src/progress/input.rs)。
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

## baseline失敗時の保存済み出力

`run` と `verify` は失敗またはプロセスの時間切れとなったbaselineについて、cleanup前に保存済みの標準出力・標準エラーをstderr診断へ逐次出力する。`--max-output` の末尾保持上限に従い、保持量・観測量・切詰め有無とraw byte offsetを示す。非UTF8は置換文字、端末制御文字はエスケープで表す。stdoutのスキーマは変えない。保存にはstderrのリダイレクトを使う。キャンセルや出力先の詰まりで総時間制限に達すると出力は途中で止まりうる。[^issue-464]

[^issue-464]: [原文](../../superpowers/specs/2026-09-14-issue-464-baseline-output-design.md)。

## コピー方針変更時の再開互換性

`5e631ef`ではinclude/excludeがfingerprintへ含まれず、コピーされる補助ファイルが変わっても旧条件のkilled/survivedを再利用した（#563）。[監査の証拠と限界](../audits/resume-copy-2026-09.md)に、設定変更の4反例と据置き・出力上限変更の正例を記録した。修正時にはコピー設定の比較方法とfingerprint schemaの更新を決める。[^resume-copy-audit]

[^resume-copy-audit]: [コピー方針とsession再開](../../audits/2026-09-15-resume-copy/README.md)。

## 2026-09-24: Issue #563の修正

2026-09-24の#563修正では、include/excludeの順序付きリストをfingerprintへ追加し、schemaを8へ進めた。パターンの表記と順序を保持するため、同じ意味でも異なる表記の設定では新規runになる場合がある。旧fingerprintの結果を新しいrunへ混在させず、jobs/max-outputだけの変更は従来どおり互換とする。[^issue-563-repair]

[^issue-563-repair]: [修正設計](../../superpowers/specs/2026-09-24-issue-563-design.md)。
