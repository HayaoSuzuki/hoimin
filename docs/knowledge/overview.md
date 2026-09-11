---
type: Reference
title: このカタログの範囲と読み方
description: 設計・監査・実装の関係、資料の版と証拠レベル、更新方法を示す。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: readme
  resource: ../../README.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: modified
  sha256: 6f6a014526b10fa17386463390d61c4989350de9908236094c82bc21068eb3e6
- id: development
  resource: ../development.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: modified
  sha256: 0cd970d39487cbdcf589949acacfe39f82d9878df459a3078abcd91639faa40b
- id: boundary
  resource: ../superpowers/reports/2026-09-11-boundary-contract-audit/README.md
  working_tree: untracked
  sha256: 6afdc6ecff7fac290da8e43f3d27f5af25718405fcd86e0f493ff51280df1d40
- id: progress
  resource: ../superpowers/reports/2026-09-11-progress-input-lean.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: japanese-style
  resource: https://gist.github.com/k16shikano/fd287c3133457c4fd8f5601d34aa817d/8f2d57610a73efc97d743c9b0b0ecb1002e09fa4
  revision: 8f2d57610a73efc97d743c9b0b0ecb1002e09fa4
- id: workflow
  resource: ../okf-workflow.md
  working_tree: untracked
  sha256: 223711d00bd253408e373e1df7839435363c5756e9e0f37771d675707b5a57f4
---

# 目的と対象

hoiminは、Pythonソースに小さな変更を加え、テストでその変化を検出できるか調べるmutation testing用のCLIである。対象を絞って変異候補を生成し、隔離した作業コピーで実行する。結果はプログラムで読み取れる形式で出力し、選んだ候補を再検証できる。[^readme]

このOKFバンドルは、設計と監査をトピックごとにまとめた文書群である。まず[設計](design/index.md)で機能ごとの責務と条件を読み、[監査](audits/index.md)で検証の根拠と限界を確認する。個別Issueの原文は元の場所に残してあり、[設計書一覧](references/design-documents.md)と[監査・報告一覧](references/audit-documents.md)から探せる。

# 収録範囲

- 要約: 全体構成、解析器、対象選択とplan/verify、実行・資源・終了処理、sessionと出力。
- 監査: Leanの証明と対応検証、2026年7月Rust監査、9月11日境界監査、後続progress入力修正。
- 原文一覧: `docs/superpowers/specs/`、`docs/superpowers/reports/`、`docs/audits/` の全Markdown。主要資料だけを本文で要約し、一覧の各項目はファイル名・先頭見出しに基づく案内とする。
- `docs/superpowers/plans/` は実施手順の履歴として原位置に残す。必要な作業の詳細は対応する設計書から原文を参照する。

# 版と証拠の読み方

作成日: 2026-09-11。`catalog_revision` はカタログ作成時に参照したリポジトリのコミットを記録する。過去の監査対象は、それぞれのページの `audit_revision` または本文に別記する。

`sources[].revision` は出典ファイルを参照した版、`working_tree` は作成時の追跡状態である。未追跡・変更済み資料には内容のSHA-256を記録する。これらと `catalog_revision`、`audit_revision` はこのカタログ独自の拡張であり、OKF標準フィールドではない。

9月11日の境界監査一式は、作成時にGitの追跡対象になっていなかった。リンクはこの作業ツリーでは読めるが、参照コミットをcloneした環境には存在しない。共有する際は元資料を含めるか、参照先を共有済み資料へ置き換える必要がある。[^boundary]

| 記述 | 意味 |
| --- | --- |
| 設計で採用 | 当時の判断。実装済み・現行仕様とは限らない |
| 実装を静的確認 | 参照コミットの該当コードを読んだ。実行成功を意味しない |
| 元報告で実行確認 | 記載された版・環境・入力に限る過去の結果 |
| モデル内の証明 | Leanモデルの仮定の下で成立する性質 |
| 対応検証 | 期待値付きの事例集（corpus）を実装へ適用するテストコード（adapter）で、指定した観測を比較した結果 |

Leanモデルの証明とRustの対応検証は、それぞれの範囲を明示して読む。[^development] また、同じ機能に関する報告でも時点を区別する。たとえば進捗比較を行うprogressへの入力は、9月11日の監査で課題に挙がった後、別報告で修正と対応検証が記録されている。[^boundary][^progress]

# 初回整理と日本語点検時の確認（2026-09-11）

資料と関連実装を読み、OKF構造、出典・脚注・リンクを検査した。Rust/Python/Leanの再実行、GitHub Issueの現在の状態確認は行っていない。各概念は内容の検証を完了していないため `status: draft` を保ち、過去のテスト成功や文章の点検だけで `verified` を付けていない。

以下は開発手順を組み込む前の、日本語表現の点検後にPyYAML 6.0.3とローカルの構造・参照検査で得た結果である。件数はその時点の値であり、継続更新後の総数ではない。公式の検証プログラムは使っていない。

| 検査 | 結果 |
| --- | --- |
| OKF対象 | 12概念・4索引、計16 Markdown |
| YAML・予約ファイル構造 | 全件成功 |
| 出典と脚注 | 306出典エントリのID・脚注対応を確認。ローカル出典305件は参照した内容との一致も確認 |
| ローカルリンク | 延べ606リンクの参照先が存在 |
| 原文一覧 | 設計160件・監査報告91件が対象ディレクトリのMarkdown集合と一致 |
| 導線 | 入口から全16ページへ到達可能 |
| 未追跡資料 | 出典に使った8ファイルのSHA-256が作成時の内容と一致 |

リンク検査はこの作業ツリーを対象とした。文章規範は別途公開Gistから取得し、リビジョンを記録した。外部Webページの到達性を一括検査したり、原文中の全リンクを検査したりはしていない。

継続開発では[OKFを使った開発手順](../okf-workflow.md)に従い、作業開始時に関連する概念と出典を参照する。契約・判断・検証範囲が変わったら対応する概念を同じ変更で更新し、新しい設計書・報告は原文一覧にも追加する。[^workflow]

バンドル外への相対リンクは同じリポジトリを前提とする。文書群を共有する際はリンク先の元資料も必要になる。検索サービスへの索引登録は別途設定する。

# 日本語表現の点検（2026-09-11）

日本語技術文書の文章規範のリビジョン `8f2d57610a73efc97d743c9b0b0ecb1002e09fa4` を使い、本文・見出し・索引の説明を点検した。用語を定義する順序、段落ごとの話題、推量や未検証条件の保持、翻訳調の比喩、過度な省略を確認対象とした。[^japanese-style]

主な修正は、説明のなかった実装用語を日本語で定義することと、設計判断・静的確認・過去の実行結果を別々の段落にすることである。プロセスとディレクトリの双方に使われていたrootは対象を明示し、Leanの事例集と比較用テストの役割も説明した。コードの識別子、出典の版、監査対象コミット、原文一覧の引用見出しは保持した。

修正後にYAMLと参照関係を再検査し、上表に結果を記録した。出典306件のうち1件は、この点検で追加した日本語文章規範である。

[^readme]: [README.md](../../README.md)。
[^development]: [development.md](../development.md)。
[^boundary]: [README.md](../superpowers/reports/2026-09-11-boundary-contract-audit/README.md)。
[^progress]: [2026-09-11-progress-input-lean.md](../superpowers/reports/2026-09-11-progress-input-lean.md)。
[^japanese-style]: [日本語技術文書の文章規範](https://gist.github.com/k16shikano/fd287c3133457c4fd8f5601d34aa817d/8f2d57610a73efc97d743c9b0b0ecb1002e09fa4)。

[^workflow]: [OKFを使った開発手順](../okf-workflow.md)。
