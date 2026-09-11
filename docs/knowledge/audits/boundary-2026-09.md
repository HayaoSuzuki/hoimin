---
type: Audit
title: 2026-09-11 境界条件の横断監査
description: 構文・選択・実行・保存の監査結果と、静的確認・未検証の範囲を残す。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
audit_revision: 623dd808612dbc34775e16814845eec0bc52dff9
sources:
- id: summary
  resource: ../../superpowers/reports/2026-09-11-boundary-contract-audit/README.md
  working_tree: untracked
  sha256: 6afdc6ecff7fac290da8e43f3d27f5af25718405fcd86e0f493ff51280df1d40
- id: analyzer
  resource: ../../superpowers/reports/2026-09-11-boundary-contract-audit/analyzer.md
  working_tree: untracked
  sha256: 30ec2c78aea9b7f4fd9c0da655fae9096ff7ed91499851f2da619fb1c483b1ea
- id: selection
  resource: ../../superpowers/reports/2026-09-11-boundary-contract-audit/selection.md
  working_tree: untracked
  sha256: ba32a5d36f8e13c3a8237fa853d9e16f5aa733dcf14add78e2ee6f156dbafcc7
- id: runtime
  resource: ../../superpowers/reports/2026-09-11-boundary-contract-audit/runtime.md
  working_tree: untracked
  sha256: 3e338e5144cd0ce0df95877599cad053306eb7676759dfe64721b17ec4ad3907
- id: session
  resource: ../../superpowers/reports/2026-09-11-boundary-contract-audit/report-session.md
  working_tree: untracked
  sha256: b30e77536b3b8fcd06e51bfb95ecadd8b6c254089f280f9cfecaf400ccd28246
- id: evidence
  resource: ../../superpowers/reports/2026-09-11-boundary-contract-audit/verification-results.json
  working_tree: untracked
  sha256: b337a084cb71de84250f153cde6b50b34be71e467a7e7d268bc46dcdcc872022
- id: lean
  resource: ../../superpowers/reports/2026-09-11-boundary-contract-audit/lean-application.md
  working_tree: untracked
  sha256: 5d1f9dacbbd186b1043a41573eb698e1a90d5f948c1419d01f7cb7fdc9a1c4b5
---

# 対象コミットと資料の保存状態

監査対象は `623dd808612dbc34775e16814845eec0bc52dff9` である。実測にはmacOS arm64・CPython 3.14.7と、同コミットのdebug/release CLIを使った。以下の結果はその版と条件での報告であり、現在も各不具合が再現するかは再検証していない。[^summary]

元報告はカタログ作成時にGitの追跡対象になっていなかった。内容を識別できるよう、各出典のSHA-256をYAMLメタデータに記録した。

# 処理をまたぐ契約と入力形状の課題

既存の試験には、境界値、性質を多くの入力で調べるプロパティテスト、実プロセス、障害注入、Leanとの対応確認がある。そのうえで監査は、ある処理で検証した前提を結果の読取り側でも確認すること、共通実装に依存しない照合先を持つこと、入力件数に加えて1行の長さや構文の深さも測ることを課題に挙げた。[^summary]

| 領域 | 当時の確認内容 | 原文 |
| --- | --- | --- |
| 解析器 | 構文上の役割、名前の有効範囲、変更するバイト範囲、候補保持、入力形状 | [解析監査](../../superpowers/reports/2026-09-11-boundary-contract-audit/analyzer.md) |
| 対象選択 | 複数指定の組合せ、順位、候補発見の打切り、72通りのplan条件 | [選択監査](../../superpowers/reports/2026-09-11-boundary-contract-audit/selection.md) |
| 実行 | 制限、並列処理・取消し・後始末、OSの動作とモデルの対応 | [実行監査](../../superpowers/reports/2026-09-11-boundary-contract-audit/runtime.md) |
| 保存・比較 | 終了判定、再利用、スキーマv2/v3、読取り時の整合性検証 | [保存監査](../../superpowers/reports/2026-09-11-boundary-contract-audit/report-session.md) |

表の各行は元監査の対象を示す。[^analyzer][^selection][^runtime][^session]

# 発見した不具合と検証基盤の改善案

| 区分 | 元報告が挙げたIssueと根拠 |
| --- | --- |
| 実行再現4件 | #484 metrics出力が元ソースを上書き、#485 マッピングパターンの重複キー、#486 ジェネリック型パラメータと組込み名の誤認、#487 資源制御モードの誤報告 |
| 静的な契約不一致1件 | #488 Windowsの制限が実行全体に適用されるという説明との不一致。Windows実機は未検証 |
| 検証基盤の改善3件 | #489 Python構文を横断して調べる事例集、#490 コマンド・保存形式間の契約表、#491 入力形状別の性能回帰検査 |

Issue番号は元報告の識別子である。現在の未解決・完了状態は確認していない。[^summary]

# 実行した試験と未検証の条件

主担当の9試験群には、成功229件・失敗0件・除外1件と記録されている。追加実験には、72通りのplan条件、候補発見を打ち切らなかった24回のrun、26回のverify、21通りのCLI設定境界がある。補助試験には重複があるため、件数を単純には合計できない。コマンドと結果は要約JSONおよび各領域の表から確認できる。[^summary][^evidence]

OS別では、Windows Job Objectと権限を委譲したLinux cgroupの実機確認は行っていない。実際のメモリ不足・空き容量不足、最大256並列worker、プロセスID再利用も範囲外である。有限の入力だけを使ったため、任意構文・全組合せ・任意入力規模の網羅性は主張していない。[^summary]

また、verifyの候補再発見に時間制限が適用される試験だけでは、manifest読込みなどを含む準備全体の時間制限と取消しを確認できない。元報告はこの適用範囲も未検証として残した。[^summary]

# 後続資料と更新条件

監査で課題に含めた #483 の集計値の整合性には、[後続修正報告](progress-input.md)がある。このカタログの参照HEADにはその修正が含まれるため、元監査の未解決一覧は当時の状態として読む。元監査のLean活用案でも、実施済み証明と、今後作成する実装比較用テストの計画を区別する。[^lean]

対応する修正や追加試験が報告されたら、元結果を残したうえで後続報告を関連付ける。共有前には、未追跡の原資料を受け手が読めるかも確認する。

[^summary]: [README.md](../../superpowers/reports/2026-09-11-boundary-contract-audit/README.md)。
[^analyzer]: [analyzer.md](../../superpowers/reports/2026-09-11-boundary-contract-audit/analyzer.md)。
[^selection]: [selection.md](../../superpowers/reports/2026-09-11-boundary-contract-audit/selection.md)。
[^runtime]: [runtime.md](../../superpowers/reports/2026-09-11-boundary-contract-audit/runtime.md)。
[^session]: [report-session.md](../../superpowers/reports/2026-09-11-boundary-contract-audit/report-session.md)。
[^evidence]: [verification-results.json](../../superpowers/reports/2026-09-11-boundary-contract-audit/verification-results.json)。
[^lean]: [lean-application.md](../../superpowers/reports/2026-09-11-boundary-contract-audit/lean-application.md)。
