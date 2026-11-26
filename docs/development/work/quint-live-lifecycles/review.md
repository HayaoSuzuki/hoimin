# 検証とセルフレビュー

## ディスク観測

| 回 | 実装レビュー | テストレビュー |
| --- | --- | --- |
| 1 | 公開snapshotから理由・測定値・時間・messageを投影 | 正例3ケースの未実装エラーによる失敗を確認 |
| 2 | snapshot.stop.secondaryとsnapshot.secondaryの一致を確認 | 最初のadapterの誤った空リスト仮定をPBTが検出 |
| 3 | 最初の停止証拠と後続診断の順序を維持 | 同時閾値・同値再通知・時間だけ異なる測定を重点履歴にした |
| 4 | typed dispatchから公開イベントへの変換だけを行う | API呼び出し省略はState invariant failedのみを受理 |
| 5 | 25-byte decoderでu64測定値と時間を切り詰めない | 固定ITF→seed PBT→byte PBT→同じdriverのfuzzを接続 |

software-design-philosophy: 検証の責務境界は8/8、10/10。失敗項目なし。期待値生成をRust adapterへ複製せず、製品APIも変更しない。入力の反復・分岐はdriverへ置き、個別ケースはAAAとparametrizationを使う。

TLCは934到達状態、4959ms、peak RSS461792KiB。負例は3768ms、exit1かつ意味的反例を確認した。20秒/RSS2048MiBの制限は変更していない。Connectはディスク6ケース、既存workerを合わせた全features14ケースが成功。通常Rustは固定ITF7件＋byte PBT1件、既存disk_model7件とquint_disk16件が成功した。

初回のguard呼び出しはsample-msの指定漏れで起動できず、修正してから未実装による3失敗を確認した。初稿adapterは公開snapshotの診断複製を内部表現と混同し、正例5件が失敗した。公開snapshotの仕様を再読して修正し、縮小されたseed6607を保存した。これらの起動・adapterエラーは意味的負例の検出に数えていない。

fuzzの登録試験は初稿で期待順とmanifest順が異なり2失敗した。manifestを既存disk_lifecycleの直後へ置き、関連Python全86ケースの成功を確認した。

新fuzzerは10秒指定で11秒・241386入力を実行し異常なし。ビルドを含むguard実測は77147ms、peak RSS1162304KiB（RSS2048MiB以内）、終了コード0。入力timeout5秒、fuzzer RSS1024MiB、seed1172、max_len4096。生成corpusは固定seedから分離した。

workspace全targets/全features、Connect全targets/全features、fuzz targetのClippyが成功した。Ruff、ty、workflow lint、strict docs buildも成功。初回Clippyの指摘は未対応variantの明示とdocのbacktickで、修正後に再確認した。

容量調査で旧issue-1172 worktreeのtarget約37GiBが残っていた。動作中Cargoがないことを確認して今回の成果物も含めcleanし、ユーザー許可に従い変更のない旧worktreeを削除した。空き容量は約21GiBから61GiBへ回復した。他アプリのキャッシュは変更していない。

## 予約台帳

| 回 | 実装レビュー | テストレビュー |
| --- | --- | --- |
| 1 | reserveAmount/releaseIdを共有し期待値の重複実装を避ける | 未実装の重点ケース3件の失敗を確認 |
| 2 | wrapperのidleで内部変数を列挙せずidleBudgetへ隠蔽 | 従来の固定ITFを更新せず鮮度確認 |
| 3 | 返却ID、公開予約情報、種類別合計、エラーpayloadを観測 | 上限拒否後のID、zero予約、重複/未知IDを別シナリオで確認 |
| 4 | next_idは成功応答件数、releasedは公開予約の不在から導く | release成功だけを偽装する負例でState比較の検出力を確認 |
| 5 | 有限入力範囲0–4と実返却IDを照会しprivate fieldを使わない | parametrizeからseed PBTへ展開し既存journal PBT/fuzzを保持 |

software-design-philosophy: 最終の8診断は8/8、10/10。wrapperがidle時の内部変数更新を知る初稿は、情報隠蔽の診断が未達だった。idleBudgetを元のモデルに移し、変数一覧をwrapperから除去した。未達項目は解消した。

初稿のReserveError variant名の誤りはコンパイル時に修正した。次にルートで定義したdispatchが名前空間なしで出力される点を実トレースで確認し、Config.nondetを修正した。seed14015を保存した。いずれも意味的負例検出には数えていない。

修正後は台帳の通常Connect5ケースが成功（8547ms、peak RSS326688KiB、全体guard300秒/RSS2048MiB以内）。入力は8コマンド、最大4成功予約、3種類・各上限2。reuse/zeroの重点ケースとseed PBT、missing releaseの意味的負例を含む。無制限の予約ID空間や並行アクセスの証明とは扱わない。

最終のConnect全featuresはworker8・ディスク6・台帳5の19ケースが成功（テスト25.81秒、ビルドと待ちを含む39744ms、peak RSS593664KiB）。台帳5とディスク6は通常profileでも成功した。通常workspaceの関連ITF/PBTは5 suite合計48ケースが成功。Connect全targets/全featuresのClippyも成功した。

全registry（17設定・11負例・109固定ITF）の最終実行が終了コード0で成功した。従来の固定履歴は変更していない。Python全体は2796成功・35スキップ（83.73秒）、Ruff check/format・ty・workflow lint・strict docs buildも成功した。

最後のテストレビューで、ディスクbyte PBTの最終Assertをテスト本体にも明示した。共有driverは途中の各状態にも同じ証拠の一意性を検査する。関連ITF/PBTとライブ検証をこの整理後にも再実行する。

最終整理後も関連48ケースと全features Connect19ケースが成功（Connectテスト28.15秒）。fuzzを再実行し、5秒指定で6秒・85025入力、異常なし。ビルドを含む全体は66004ms、peak RSS1848496KiB、20/300秒とRSS2048MiBの既存guardを緩和していない。新しいseed PBTと既存の独立journal PBT/fuzzを併用した。
