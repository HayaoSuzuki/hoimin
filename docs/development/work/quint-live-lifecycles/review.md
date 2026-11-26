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
