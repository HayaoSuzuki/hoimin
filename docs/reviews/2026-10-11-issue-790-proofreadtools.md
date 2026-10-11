# #790・#776・#777のセルフレビューと検証

作業ブランチは `research/issue-790-proofreadtools`。
設計と計画の承認を受け、実装は同じブランチで進めた。
既存の `.idea/` は作業対象外とした。

## 設計段階の5回

[設計書](../superpowers/specs/2026-10-11-issue-790-proofreadtools-design.md)の末尾に各回の記録がある。
対象Issueと外部入力面、一次変異の範囲、oracle、実適用、費用と終了条件を順に確認した。

## 計画段階の5回

[実装計画](../superpowers/plans/2026-10-11-issue-790-proofreadtools-plan.md)の末尾に各回の記録がある。
設計の網羅、先行失敗テスト、既存インターフェース、誤成功、作業の比例性を順に確認した。

## 実装段階の5回

1. **候補空間と公平性。** ASTの既存オペランド・呼出し引数・演算子から単一編集を作ることを確認した。無作為と誘導は同じ18候補を使う。既存operatorの6候補との差は空間拡張の効果であり、誘導探索の効果とは分ける。有限集合のindex選択を、無制限のプログラム合成と説明しない。
2. **生成と縮小。** ValueTreeの初期値が実際の候補を指す契約テストを追加した。縮小順序は式のASTノード数、replacement長、文字列順。縮小失敗時の `complicate()` の戻り先も評価するよう制御を修正した。生存と有効な差の両方を再確認し、縮小の起動を32件・60秒へ算入する。大域的最小性は保証しない。
3. **oracleと無効な観測。** Hypothesisが差とゼロ除算を別々の失敗として発見するとExceptionGroupになる不具合を再現した。無効観測をproperty失敗として投げず診断として記録し、混在した候補を成功に数えない。有限floatの型・値を比較する。生成した期待値はcharacterizationとして明示する。
4. **実適用とimport。** editable installの元checkoutを誤って読む危険を確認した。各子プロセスのimport rootをworkerへ向け、実source pathを照合し、元・変異を各2プロセスで観測する。生成テスト追加後の新しいworkerとplanを作る。さらに `--fingerprint-include tests/**/*.py` と生成テストのfingerprint存在確認を追加した。ディレクトリにも一致する `tests/**` は実行時に拒否されたため、Pythonテストのファイルに限定した。
5. **資源とcleanup。** Windows Job Objectによる1GiB・16プロセス・1MiB出力・残時間の上限を使う。各候補前に10GiBの空き容量を確認する。切詰め出力・出力終了未確認・timeoutをkillから除外する。元checkoutの照合、worker削除、子孫回収、出力drainの全成功後だけ結果を保存する。

## テスト段階の5回

1. **RED/GREENの根拠。** 最初のRust/Pythonテストは未実装モジュールで失敗した。その後、混在した無効観測、CrossHair引数の解析、候補縮小、出力切詰めの判定について、それぞれ先に失敗を確認して修正した。失敗理由はログで確認し、環境エラーと振る舞いの失敗を分けた。
2. **負例と誤成功。** 同一関数を等価と断定しない、例外・NaN・異型値を差に数えない、未知checkoutを拒否する、既存出力を上書きしない、timeout/import error/未完出力をkillに数えないことを確認した。任意の非決定的コードへの一般化は今回の対象外。
3. **入力領域。** CrossHairのdiffbehaviorはdocstring preconditionだけでは入力を制限しないことを実装から確認し、両関数へ同じ明示guardを付けた。空文字と長さ33を除外する回帰テストを追加した。出力はAST literalとしてだけ読み、呼出し式を拒否する。対象は長さだけに依存するため、得られた文字列を同じ長さのASCIIへ投影して実モジュールで再検証する。
4. **実importとテスト収集。** 抽出関数の差だけでは成功にしない。独立した通常CPython観測に加え、新しいworkerで生成テストが `1 passed`、変異でAssertionErrorになることを確認する。skip/xfailを成功として扱わない。軽量suiteの結果は単独生成テストの成功と分けて報告する。
5. **費用と再現性。** seedに加えsource hash、編集span・replacement・候補ID、入力、生成テストhash、各反復の件数・起動回数・wall timeを保存する。既存の資源制限・子孫清掃テストは複製しない。モデル取得・heavy extra・全OS実験を追加せず、Windows/Python 3.14の小実験に限定する。

## 環境上の問題と判断

- サンプルregistryがnumpyを無条件importするため、コア依存だけではbaselineが失敗した。サンプルのdev依存として宣言されたnumpy 2.4.6だけを追加し、baseline 6件成功を確認してから探索した。
- Windowsのpytest出力がcp932になったため、子プロセスへ `PYTHONUTF8=1` と `PYTHONIOENCODING=utf-8` を指定した。文字化けした出力を成功扱いしない。
- 実行中のWindows test executableは再リンクできない。実験終了を待ってビルドした。別のビルド領域を増設してディスクを消費する方法は採らなかった。
- 第一関数に実際の生存候補があったため、第二関数への拡張は不要と判断した。

最終測定、独立レビュー、清掃の結果は[実験報告](../superpowers/reports/2026-10-11-issue-790-proofreadtools.md)に記録する。

## 独立レビューの反映

別のレビュアーがブランチ全体を確認した。
timeout時に次のarmへ進む経路と、採用した縮小結果を最終照合へ選ばない経路について、失敗するテストを先に実行し、修正後7件のRust契約テストが成功した。
負の結果で比較対象がない場合も、armの証拠を失わず診断を保存する。
実験時のgitによるcheckout確認もProcessHandlerへ移し、10秒・1MiBの上限を適用した。
元の直接git helperは非repositoryを使う小さい契約テストだけに残る。

全体suiteはnox不足、次にOpenBLASのメモリ確保失敗を診断した。
sample lockのnoxを追加し、1GiB上限を維持したままnative threadを各1へ制限した最終確認は5,053成功・PDF依存による6 skip・slow53除外だった。
最終候補のhash、具体入力、新テストのfingerprint、元成功・変異assertion失敗、清掃完了は[機械可読の証拠](../superpowers/reports/issue-790/evidence.json)へ保存した。
独立レビュアーは最終の文書・証拠・知識カタログも照合し、未解決の要対応指摘なしと判断した。
