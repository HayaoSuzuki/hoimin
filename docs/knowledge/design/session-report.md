---
type: Contract
title: session所有権とレポートの検証
description: 保存・復旧の権限、スキーマ移行、結果の生成側と読取り側の検証責務を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
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
- id: issue-484
  resource: ../../superpowers/specs/2026-09-11-issue-484-metrics-destinations-design.md
  working_tree: untracked
  sha256: b146daf296d69374ad01ac86d5a5becd30b721b1983d5a54507cd0f1750ef3e7
---

# 実行単位ごとの所有権

sessionは、一回の実行（run）の結果を保存・再利用する仕組みである。保存処理を担当する `SessionHandler` は、`begin` または成功した `load` で、そのrunを操作する所有権を得る。[^ownership]

結果保存の `persist` と再開時の結果検索 `lookup` には、要求されたrunの所有権が必要である。他のrunを所有していても代用できない。`finish` で所有権を解放した後は、未完了のrunでも再取得が必要になる。現在の実装でも、DB処理より前に `require_ownership` を呼ぶことを静的に確認した。[^ownership][^session]

所有権がなければ、保存は `session.persist.owner`、検索は `session.lookup.owner` で先に拒否する。そのため、破損DBやトランザクション失敗を調べるテストデータ（fixture）は、先に正当な所有権を確立してから障害を起こす必要がある。所有権がないままでは、試験したいDB処理へ到達しない。[^ownership-report]

Issue484の設計では、metrics出力によるsession DB、SQLite副ファイル、所有権ロックの置換も衝突検査の対象とする。WALによる復旧を上書きの安全性の根拠にせず、使用中の具体的なパスを保護する。別のリンクエントリを置き換える場合と、DBやロック自体のエントリを置き換える場合を区別する。[^issue-484]

保存先の同一性を確定できない場合は、実行結果を維持してmetricsの保存を見送り、終了時に `metrics.write` で通知する。未作成の親ディレクトリや対応範囲外の別名もこの扱いとし、既知の衝突をbaseline前に拒否する場合と区別する。[^issue-484]

# スキーマ移行の競合と検証範囲

スキーマ移行の監査では、DBの書込みロック取得後の版再確認、失敗時に変更をまとめて取り消す処理、旧データの保持、未対応の新しい版の扱いをモデル化した。公開APIで二つのopenを同時に始める試験では、両者が古い版を読んでからロックを取ったかまでは確定できない。この特定の実行順序を要求するケースは、モデル内だけで確認する `model-only` に分類している。[^migration]

一方、run所有権の修正報告では、macOSとLinuxの非rootユーザー環境で、Leanの期待値と公開APIの結果を比較する20件の `strict` ケースが一致した。有限の実行順序を調べる探索は深さ4までであり、深さ8は当時の時間制限で完了していない。これらの結果が示す範囲に、Windows・実際のクラッシュ・SQLite自体の正しさは含まれない。[^ownership-report]

# 結果の生成と読取りの責務

JSON Schemaは結果の構造を定める元資料として保持する。ただし、構造上正しいJSONにも、値どうしが矛盾した結果は入り得るため、読取り処理で意味上の整合性も検査する。現在のprogress読取り処理は、件数検査後に `validate_summary_coherence` を呼び、完了状態と終了コードが整合するか確認する。[^schema][^reader]

この集計値の検査の詳細は[progress入力の後続修正](../audits/progress-input.md)を参照する。個々の変異候補の終了理由・出力欄や、全履歴の整合性には別の検証が必要である。

# 再確認条件

run所有権、再利用条件、結果置換、DBスキーマ、集計値と終了コードの規則、JSON読取り処理を変更したら更新する。保存・読取り・比較の各処理で同じ前提が維持されるかを調べる。旧報告にある未解決という記述は、その報告の時点として扱う。

[^ownership]: [2026-09-07-issue-363-session-ownership-design.md](../../superpowers/specs/2026-09-07-issue-363-session-ownership-design.md)。
[^session]: [mod.rs](../../../crates/hoimin-cli/src/session/mod.rs)。
[^ownership-report]: [2026-09-07-issue-363-session-ownership.md](../../superpowers/reports/2026-09-07-issue-363-session-ownership.md)。
[^migration]: [2026-08-11-lean-schema-migration-concurrency-audit.md](../../superpowers/reports/2026-08-11-lean-schema-migration-concurrency-audit.md)。
[^reader]: [input.rs](../../../crates/hoimin-cli/src/progress/input.rs)。
[^schema]: [run-result.schema.json](../../json-schema/run-result.schema.json)。

[^issue-484]: [2026-09-11-issue-484-metrics-destinations-design.md](../../superpowers/specs/2026-09-11-issue-484-metrics-destinations-design.md)。
