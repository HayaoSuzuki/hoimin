# 状態機械検証拡張のレビューと実行記録

2026-11-25。初期導入 `cad49400` を `test/quint-state-machines` にコミットした後の拡張。
対象と対応範囲は [設計](design.md)、手順は [計画](plan.md)、再実行は
[formal/quint/README.md](../../../../formal/quint/README.md)。以下の各工程を5回ずつレビューした。
software-design-philosophy の8診断を使い、対象領域ごとに判断した。

## 設計の5回レビュー

| 回 | 検討した欠落・複雑性 | 判断・修正 |
| --- | --- | --- |
| D1 | 予算台帳の探索数だけを増やすと、異なるプロトコルの不具合は扱えない。 | 4予約の BudgetExtended、公開 ReportSequence、内部 Completion の3領域へ拡張。 |
| D2 | 公開 API の照合と内部 fixture を同列に扱うと証拠の意味が不明になる。 | Completion は実際の4台帳メソッドへ対応する内部 fixture と明記。public transition と OS 動作は対象外。 |
| D3 | 完了台帳の floor/gaps 更新を期待値へ複製すると同じ誤りを見逃す。 | Quint は完全な consumed 集合から prefix を導出し、PBT は slot journal を使う。本番の gap 連結ループを oracle にコピーしない。 |
| D4 | 4本のランダム履歴では完了成功・誤種別の出現が少なかった。 | モデルの同じ遷移を使い、入力順序だけを指定する interleavedStep / gapStep / wrongKindStep を追加。期待状態の別実装を作らない。 |
| D5 | モデルごとのスクリプトと namespace 対応が増えると知識が分散する。 | 一つの registry/runner と ITF reader に集約。状態変数の短縮名衝突を拒否。8診断で8/8、10/10。残る失敗診断なし。 |

評価は今回の追加モジュールの責務・入口・情報隠蔽・契約・レビュー・知識の所有・境界説明・設計改善への作者の判断であり、リポジトリ全体の点数ではない。

## 計画の5回レビュー

| 回 | 確認 | 判断・修正 |
| --- | --- | --- |
| P1 | 各成果物が単独で確認できるか。 | 先に初期導入をコミットし、予算拡大・レポート・完了台帳を別の検証単位にした。 |
| P2 | 資源制限が複数領域でも保持されるか。 | 20秒・RSS 2048 MiB、heap 1 GiB、TLC 1 worker を継承して順次実行。拡張モデルは実測してから維持を決めた。 |
| P3 | contracts profile が無効入力を panic に変える点。 | public validate のエラーを両 profile で照合。通常ビルドでは invalid observe も呼び、後続 probe で隠れた状態変更を確認。 |
| P4 | 登録済みモデルの後半や指定シナリオが CI から漏れないか。 | runner の複数モデル・後半 fixture・後半 scenario の鮮度拒否を動作テスト。CI で内部 fixture の unit tests も実行。 |
| P5 | 反例検出能力と証拠の区別。 | 各 TLC 負の対照に加え、隔離 Rust コピーへの5種類の mutation を計画・実行。8診断で8/8、10/10。未解決の失敗診断なし。 |

## 実装の5回レビュー

| 回 | 発見・確認 | 修正・結果 |
| --- | --- | --- |
| I1 | source-only のモデル提案を実ツールで確認。 | Quint の予約語 run を runId に変更。型付き action パラメータへ bool 戻り値注釈を補足し、typecheck を通した。 |
| I2 | 0.33.1 に現在の --observation は存在しない。 | 探索用試行は Unknown argument で失敗したため採用せず、pinned 版で動く共通 action と入力 schedule を実装した。 |
| I3 | ITF のモデル prefix が異なると元の Budget 専用 reader は読めない。 | namespace を消す reader を導入。初稿で短縮名3ケースと衝突拒否1ケースが失敗することを確認後、正常化と衝突拒否を実装。 |
| I4 | CI の run 行末の :: が YAML として無効だった。 | filter の末尾 :: を除去。workflow lint と関連テストを再実行。新規 fuzz target は既存 manifest から自動実行される。 |
| I5 | 本番コードのテスト用拡張、不要な API、期待値の漏洩を確認。 | 本番差分は test-only child module 登録だけ。新設の製品 API なし。Report の公開エラー照合と Completion の内部状態照合を分けた。8診断で8/8、10/10。 |

## テストの5回レビュー

| 回 | 確認 | 結果 |
| --- | --- | --- |
| T1 | 1関数1履歴、parameterization、AAA。 | リテラル履歴・ITF は rstest の独立ケース。テスト本体の手書きループ・分岐を避け、driver 内へ置いた。 |
| T2 | 長い履歴と有限モデル外の値。 | 3領域の PBT を各256ケース・最大127操作で実行。Report は任意 u64 のイベント番号・identity、Completion は未知 MAX ID と retire 後の新規 issue を含む。 |
| T3 | 乱数だけで重要な経路を通ったと判断していないか。 | 指定入力の ITF により2 mutant 逆順完了、4要求逆順完了による gap 回収、wrong completion と generic failure を必ず実行。 |
| T4 | モデル内だけで閉じていないか。 | 隔離した実装から終了制約、非単調番号拒否、retired 優先、gap 削除、予算上限チェックを壊し、各テストの失敗を確認。 |
| T5 | 全体検証、fuzz、CI、文書。 | 下記の実行記録へ結果を記す。初回 mutation 実験の lib test は既存 Lean corpus がコピーされておらず compile error だったため、これを検出成功に数えず、必要な corpus を含めてやり直した。 |

## 有限モデルの実行

4設定の TLC が成功し、3種類の負の対照が不変条件違反を検出した。基本モデルに加えて12本のランダム履歴と3本の指定履歴を追加し、計19本を再生成・比較する。

| モデル | 到達状態 | 設定ごとの制限付き実行時間 | peak RSS |
| --- | ---: | ---: | ---: |
| Budget | 712 | 約5.0秒 | 約499 MiB |
| BudgetExtended | 58,467 | 約6.8秒 | 約723 MiB |
| Report | 4,987 | 約4.8秒 | 約678 MiB |
| Completion | 3,766 | 約4.9秒 | 約639 MiB |

時間は typecheck と別に、変換・検査・子終了確認を含む guard の計測。TLC の fingerprint による有限状態探索であり、Rust の無限履歴の定理証明ではない。設定間で重なる状態もあるため、表の合計を互いに異なる実装状態の数とは解釈しない。

## 実装 mutation の検出

作業ツリーは変更せず、一時コピーで本番実装を変更した。通常成功すべきテストが assertion で失敗したことを確認し、compile error は成功に数えない。

| 変更 | 失敗した検査 |
| --- | --- |
| active mutant を残した RunFinished を許可 | Report の ITF 2件、リテラルと PBT |
| sequence <= previous を < に変更 | Report の ITF 4件、リテラルと PBT |
| retired 優先分岐を除去 | Completion の ITF、リテラルと PBT：計6失敗 |
| gap の remove を contains に変更 | Completion の指定 gap 履歴、逆順リテラル、PBT：計3失敗 |
| check_budget_amount を除去 | 予算の基本・拡大 ITF 計8件、リテラルと PBT |

2 agents へ領域を分担したが、どちらも tool host 不在で checkout を読めなかった。提供された source-only モデル提案を作者が適用・型検査・修正・実行した。独立した checkout 全体レビューを完了したとは主張しない。

## 最終チェック

Python 全体は2779成功・35スキップ（169.13秒）。contracts 有効の Report fuzz も1000入力を異常なく実行。Rust 全体は `RUST_TEST_THREADS=4 cargo test --locked --workspace --no-fail-fast` により241 suite、18742成功・失敗0・62 ignored。対象34ケースと内部完了台帳11ケースは contracts 有効でも成功。workflow lint、ruff check / format、ty check、workspace clippy 全 targets / features、fmt、ドキュメントの strict build も成功。通常 profile の report_lifecycle fuzz は15秒指定で90147回（実時間16秒）、異常なし。別ディレクトリへ生成 corpus を保存した。各入力 timeout 5秒、RSS 上限1024 MiB。
