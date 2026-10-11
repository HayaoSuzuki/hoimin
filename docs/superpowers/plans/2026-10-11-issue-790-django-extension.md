# Djangoでの探索比較

ユーザー承認済みの短い設計を具体化する。製品機能は変更せず、PR #791の研究を拡張する。

## 設計

Djangoの固定commit `dab0a5c47f159eb58472110dcbc10bd4a9b01adb` を用いる。
対象は `django.utils.numberformat.format`、`django.utils.timesince.timesince`、`django.utils.duration.duration_microseconds`。
ネットワークサービスは使わず、Djangoの公式runnerと既存の関連テストを使う。
Python 3.14、jobs=1、コピー8GiB、空き10GiB、メモリ1GiB、子プロセス16、出力1MiBを維持する。

既存operatorの候補群と、同じAST単一編集からなる無作為・誘導候補群を比較する。
対象3関数を合わせたpoolから選び、各modeをseed 790, 776, 777, 748, 750で実行する。
各armはwall 30秒・最大12評価。候補の既存suite評価と生存候補への入力生成も予算に含む。
無作為・誘導は同じpool、入力領域、観測、予算を用いる。各評価は最大10秒で、残時間を上限にする。
誘導は生存候補での有効な観測差とoperator種類の新規性を報酬にする。製品への採用や優位性は結果から判断する。
例外・timeout・出力未完了をkillと混同しない。異常終了時は探索を停止して清掃する。

数値整形は有界の整数・小数文字列と整形引数、timesinceは明示的な2日時とdepth、durationは有界のmicroseconds整数を生成する。
入力は開発者が固定する。任意コードや外部入力を受け付ける製品機能は追加しない。
Hypothesisで縮小した入力があっても、元・変異の実moduleを新しい子プロセスで各2回照合するまでは有効な証拠としない。
各対象で得た異なる生存候補から最大1件ずつテストを生成し、新plan・新workerで元成功と変異assertion失敗を確認する。
元の出力はcharacterizationであり、人の仕様レビューなしでupstreamへ採用しない。

## 計画

1. 関数選択を既存AST generatorへ引数として渡せるようにし、既存proofreadtoolsの契約を保持する。
2. 既存ProcessHandlerとworkerを共有し、Django公式runner・生成入力・実module照合用の小さい補助コードを加える。
3. baseline・既存候補plan・15armを実行し、件数、終了理由、候補ID、hash、wall、process数、全ての試行を記録する。
4. 生存候補があれば生成テストを照合。なければ負の結果を保存し、未発見を等価性と呼ばない。
5. 関連テスト・厳格lint・CI一致契約を確認。資料とレビューをコミットし、cargo cleanと一時環境の清掃後PRへpushする。

## 設計セルフレビュー5回

1. 規模でなく複数関数と候補poolの拡大で前実験の飽和を避ける。
2. Django全体のmutation scoreを主張せず、選択した関数に限定する。
3. 無作為・誘導で候補集合と実行予算を等しく保つ。
4. 既存suiteが強く全killでも正しい負の結果として報告する。
5. 純Pythonの対象を選び、ネイティブビルドや外部サービスは導入しない。

## 計画セルフレビュー5回

1. 試行前に固定SHA・baseline・空き容量を確認する。
2. 既存generatorとresource handlerを共有し、製品CLIへの設定追加はしない。
3. 重要な境界（対象選択、観測型、例外とkillの区別）を先行テストで確認する。
4. arm途中の清掃失敗・timeoutは次へ進む条件にしない。
5. 実験を通常CIで自動起動せず、小さい契約だけを既存の両quality workflowへ追加する。
