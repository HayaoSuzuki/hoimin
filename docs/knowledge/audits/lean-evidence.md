---
type: Audit
title: Lean監査の証拠をどう読むか
description: モデル内証明・有限探索・Rust対応・実機確認の違いを具体例で示す。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: development
  resource: ../../development.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: state
  resource: ../../superpowers/reports/2026-08-09-lean-state-machine-oracle.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: migration
  resource: ../../superpowers/reports/2026-08-11-lean-schema-migration-concurrency-audit.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: cleanup
  resource: ../../superpowers/reports/2026-09-08-issue-342-resource-audit.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: ownership
  resource: ../../superpowers/reports/2026-09-07-issue-363-session-ownership.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: ci
  resource: ../../superpowers/reports/2026-09-08-issue-369-lean-ci-audit.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
---

# 証明・探索・対応試験・実機確認

hoiminのLean監査では、モデル内の性質を証明する作業と、Rust実装をそのモデルと比較する作業を分ける。Rustとの比較には、Leanが生成した期待値付きの事例集（corpus）と、各事例を実装の入力へ変換して結果を観測するテストコード（adapter）を使う。[^development]

| 確認方法 | 確認すること | 残る確認事項 |
| --- | --- | --- |
| Leanの定理 | 明示したモデルと仮定で性質が成立すること | Rust・OS・DBの実装とモデルの一致 |
| 有限探索・壊したモデルとの比較 | 指定した深さ・イベント集合での反例と、検査が誤りを見つける能力 | 範囲外の長さ・入力・実行順序 |
| Rust adapterとcorpus | 同じ前提を構成できる事例で公開APIの観測が一致すること | adapterが強制できない内部の実行時刻や未列挙の入力 |
| 実機確認 | 実行した版・OS・設定での結果 | 別OS・別の資源制御方式・未実行の負荷条件 |

公開APIとの比較で一致を要求するケースは `strict`、モデル内だけで確認するケースは `model-only` と記録する。どの分類かによって、実装について述べられる範囲が変わる。[^state][^migration]

# 監査ごとに異なる前提

状態機械の監査では、後始末（cleanup）の完了待ち中に二度目の停止要求を受けると、保留中の後処理要求を処理待ちから外し、別のIDで出し直す不具合を見つけた。修正後は元の後処理要求とIDを保持し、14件のstrictケースが一致した。モデル側の証明は一回の遷移に関する性質や、個別に定めた実行履歴を対象とし、到達可能な全履歴に対する帰納証明は行っていない。[^state]

スキーマ移行では、公開APIから実行順序を固定できるかが問題になる。`forced_stale_reread` は、二つの接続が古い版を読んでから書込み権限を取る順序を公開APIで強制できないため、model-onlyに分類した。同時にopenが成功した結果だけでは、その順序で動いたとは確認できない。[^migration]

資源の後始末を扱う監査では、真偽値による抽象モデルと、独立した内部Rust試験を組み合わせている。strictな対応試験や生成corpusによる比較はなく、モデルの成功とRust試験の成功はそれぞれの証拠として読む。[^cleanup]

実行結果の保存・再利用を担うsessionの所有権監査では、有限探索に含めた操作の集合に、再開時の結果検索（lookup）がない。lookupの根拠は別の定理、固定したcorpus、意図的に壊したモデルの検出結果にある。探索が安全と判定しただけでは、lookupについて説明できない。[^ownership]

# Lean実行の資源条件

開発資料は、Leanのビルド、corpusの再生成結果との一致検査、意図的に壊したモデルを見分ける感度検査を直列で行うと定める。各コマンドは時間とメモリを監視する資源guardを通す。実行時には現在の開発資料とCI設定を参照し、過去の報告にある対象数や時間上限をそのまま使わない。[^development]

CI監査報告には、キャッシュなしのビルドでのRSS（常駐メモリ）制限到達、予算統計の起動時評価による時間切れ、その後の修正と追試が記録されている。いずれも実行環境の制約を調べた記録であり、モデルと実装の意味上の不一致とは区別されている。[^ci]

# このページの確認範囲

今回は既存資料を読み、証拠の分類を整理した。新しいLean証明、corpus生成、Rust実行、CI履歴取得は行っていない。成功結果を参照する際は、元報告に記載された版と環境を使う。

モデルの前提、観測する公開APIの結果、adapter、探索深さ、操作の集合、資源制御の実装が変わったら更新する。[progress入力](progress-input.md)は、読取り処理の修正に伴って既存テストデータの分類も見直した例である。

[^development]: [development.md](../../development.md)。
[^state]: [2026-08-09-lean-state-machine-oracle.md](../../superpowers/reports/2026-08-09-lean-state-machine-oracle.md)。
[^migration]: [2026-08-11-lean-schema-migration-concurrency-audit.md](../../superpowers/reports/2026-08-11-lean-schema-migration-concurrency-audit.md)。
[^cleanup]: [2026-09-08-issue-342-resource-audit.md](../../superpowers/reports/2026-09-08-issue-342-resource-audit.md)。
[^ownership]: [2026-09-07-issue-363-session-ownership.md](../../superpowers/reports/2026-09-07-issue-363-session-ownership.md)。
[^ci]: [2026-09-08-issue-369-lean-ci-audit.md](../../superpowers/reports/2026-09-08-issue-369-lean-ci-audit.md)。
