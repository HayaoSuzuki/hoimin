---
type: Contract
title: session所有権とレポートの検証
description: 保存・復旧の権限、スキーマ移行、結果の生成側と読取り側の検証責務を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
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
---

# 実行単位ごとの所有権

sessionは、一回の実行（run）の結果を保存・再利用する仕組みである。保存処理を担当する `SessionHandler` は、`begin` または成功した `load` で、そのrunを操作する所有権を得る。[^ownership]

結果保存の `persist` と再開時の結果検索 `lookup` には、要求されたrunの所有権が必要である。他のrunを所有していても代用できない。`finish` で所有権を解放した後は、未完了のrunでも再取得が必要になる。現在の実装でも、DB処理より前に `require_ownership` を呼ぶことを静的に確認した。[^ownership][^session]

所有権がなければ、保存は `session.persist.owner`、検索は `session.lookup.owner` で先に拒否する。そのため、破損DBやトランザクション失敗を調べるテストデータ（fixture）は、先に正当な所有権を確立してから障害を起こす必要がある。所有権がないままでは、試験したいDB処理へ到達しない。[^ownership-report]

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

[^ownership]: [2026-09-07-issue-363-session-ownership-design.md](../../superpowers/specs/2026-09-07-issue-363-session-ownership-design.md)。
[^session]: [mod.rs](../../../crates/hoimin-cli/src/session/mod.rs)。
[^ownership-report]: [2026-09-07-issue-363-session-ownership.md](../../superpowers/reports/2026-09-07-issue-363-session-ownership.md)。
[^migration]: [2026-08-11-lean-schema-migration-concurrency-audit.md](../../superpowers/reports/2026-08-11-lean-schema-migration-concurrency-audit.md)。
[^reader]: [input.rs](../../../crates/hoimin-cli/src/progress/input.rs)。
[^schema]: [run-result.schema.json](../../json-schema/run-result.schema.json)。

[^issue-472]: [2026-09-11-issue-472-session-artifacts-design.md](../../superpowers/specs/2026-09-11-issue-472-session-artifacts-design.md)。

[^issue-460]: [2026-09-11-issue-460-progress-result-design.md](../../superpowers/specs/2026-09-11-issue-460-progress-result-design.md)。
