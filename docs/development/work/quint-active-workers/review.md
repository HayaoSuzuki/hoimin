# 実行中 worker と quint-connect のレビュー記録

基点 `24c1faea`、作業ブランチ `test/quint-state-machines`。作者のセルフレビューであり、独立した第三者レビューではない。設計と計画の各5回は [設計](design.md) と [計画](plan.md) に記録した。

## 実装レビュー5回

- I1: 公開 constructor と正しい前段通知で複数 ApplyMutation 待ちへ到達させた。session lookup は None を応答し、実行開始地点を揃えた。
- I2: 初稿モデルは既知の候補を drain したら即 verification としたが、実装は spool EOF まで ReadCandidate(Next) を行う。private field を合わせる修正はせず、モデルに EOF 確認の状態を追加した。
- I3: quint-connect の操作情報を typed dispatch 一つへ集約し、State=() を使わず共有 Snapshot を照合する。匿名 action の推論や別の期待状態キャッシュに依存しない。
- I4: itf の deserialize_any は BigInt を文字列にする。固定 ITF の #bigint と live decoding の数値・文字列を同じ観測 helper で扱い、seed0の回帰も保持した。switch macro に必要な anyhow も検証用 dev-dependency に限定した。
- I5: 終了出力の発行/確認を enum にまとめ、正常操作と拒否通知の構築を分けた。依存・CLI・lockfile を独立 workspace に置き、固定 backend と既存 guard 付き launcher からだけ live check を実行する。

software-design-philosophy: 追加検証の境界は8/8、10/10。失敗項目なし。責務、単純な入口、変更の局所化、抽象のコメント、レビュー、知識の所有、境界説明、設計改善を確認した。製品や無限状態の正しさの点数ではない。

## テストレビュー5回

- T1: 1関数1履歴、AAA、rstest、PBT を確認した。literal と固定 ITF は独立したケース。反復と分岐は driver に置く。
- T2: 2 worker の有限モデルを、1〜4 worker・最大95生成操作のPBT、最大128操作のfuzzへ広げる。誤った既存 worker と存在しない worker の両方、保存結果 identity、retired/duplicate/unknown/wrong completion を扱う。
- T3: 通常の逆順完了、開始確認前停止、実行中停止、保存待ち停止を固定スケジュールにした。worker の重点履歴は24操作まで生成し、最後まで到達したことを試験する。他領域の12操作上限は変更しない。
- T4: live library では4トレース/seedを再生し、PBT がseedを生成する。API呼び出しを一つ省く負例は State invariant failed に限定して検出し、CLI失敗を代用しない。既存PBTのコマンド列縮小と用途を分ける。
- T5: 任意 failure を入れ続ける liveness を主張しない。生成履歴の後に成功通知を与えたときの終了、実装変異、実fuzz、通常/全featuresと全体試験、資源・CI・文書を確認する。

## 実行結果

全16設定・10負例と102履歴の鮮度検査が成功。Workers は1232状態、WorkersSession は1703状態を全探索した。guard 実時間・peak RSS はそれぞれ5.000秒・約653MiB、5.994秒・約694MiBで、既存20秒/RSS2048MiB内。

固定 ITF と literal/PBT の追加32ケースが通常・全featuresで成功。専用 quint-connect は3スケジュール×session有無の6ケース、呼び出し欠落の意味的負例1ケース、生成seed PBT1ケースを実行する。PBTは8新規seed・最大16 shrink、各seedに4トレース。通常構成と全features構成でそれぞれ8ケースが成功。cold cache からの全features検証も44.651秒・peak RSS約625MiBで成功し、Cargo/全子プロセスの300秒・2048MiB制限内だった。

一時コピーの実装変異は以下のとおり。compile error や guard failure は検出に数えず、実際の test failure を確認した。

| 変異 | ITF 失敗数 | literal/PBT 失敗数 |
| --- | ---: | ---: |
| 停止時に保存待ちの結果を集計しない | 2 | 2 |
| 終了出力待ちの結果を停止時に二重加算 | 2 | 3 |
| 停止時の pending retirement を除く | 14 | 11 |
| 保存結果の run identity 不一致を許可 | 1 | 1 |
| 保存要求の結果を NotRun に壊す | 3 | 6 |
| 他 worker が稼働中でも最終処理へ進む | 2 | 5 |

加えて製品の ResetWorker callback を省く変異を、公式 quint-connect の逆順完了2ケースへ再生した。共有 driver の通常検査を通った後、ライブラリ自身の State 比較が State invariant failed を返し、2ケースとも失敗した。API呼び出しを省くadapter負例だけで検出能力を判断していない。

run_workers libFuzzer は pinned nightly、seed1172、15秒指定で16秒・28434入力、異常なし。入力timeout5秒、RSS1024MiB、max_len4096、最大128生成操作と成功応答の drain。生成 corpus とseedは分離した。

Python 全体は2793成功・35スキップ（134.52秒）。live launcher の境界11ケースも成功し、20秒の生成 guard と300秒の Cargo/子プロセス全体の guard、各RSS2048MiB、終了コードの伝播を確認した。

最初の Rust 全体は248 suite、18956成功・5失敗・62 ignored。5失敗はいずれも resolved_timeout の既存CLIケースで、実診断 filesystem.reserve.reached が空き9208483840 bytes、既定閾値10737418240 bytesを示した。タイムアウト検証fixtureに他のCLI試験と同じ min-free-space=1B を明示し、容量依存を切り離した。

該当ケースは `auto_timeout_is_reported_by_the_public_json_command`、`fixed_timeout_is_reported_by_the_public_jsonl_command`、`failed_baseline_does_not_report_an_auto_timeout_as_resolved`、`failed_baseline_does_not_report_a_fixed_timeout_as_resolved`、`verify_reports_the_saved_fixed_timeout`。

fixture 初稿の0はCLIが拒否する値であり、再実行でも5件が入力エラーで失敗した。有効な最小値1Bへ修正し、該当18ケースを全体再実行前に確認した。

ユーザー指定の cargo clean を、Cargo/rustc/fuzz が動作していないことを確認して実行した。workspace 35.5GiB、fuzz 3.1GiBの成果物を削除し、空き約41GiBへ回復した。ソース、lockfile、seed、固定ITF、検証ログは保持した。最終検証後にも実行する。

最終の Rust 全体は `RUST_TEST_THREADS=4 cargo test --locked --workspace --no-fail-fast` で248 suite、18961成功・失敗0・62 ignored。1B修正後の timeout fixture 18ケースも単独で成功した。Python 全体は2793成功・35スキップ、専用 quint-connect は通常/全featuresで各8ケース、cold cache でも成功。全16設定・10負例・102履歴、実装変異、実fuzz、formatter、Ruff、ty、workflow lint、strict docs build を確認した。

最終の workspace 全targets/全features Clippy も成功（47.33秒）。コミットには workspace/vendor の fmt と Clippy の必須 hook を適用し、その後 Cargo が停止した区切りで workspace/fuzz の cargo clean を再実行する。

## PR #1211: Linux CI の TLC 時間超過

ジョブ139070902989は DiskLifecycle の検証中に終了コード124で停止した。guard の記録は20443ms、peak RSS681240KiB、reason=timeout。反例ではない。修正前のローカル実測は14249ms、25546到達状態であり、20秒の制限に対する余裕が小さかった。

| 回 | 設計レビュー | 計画レビュー | 実装レビュー | テストレビュー |
| --- | --- | --- | --- | --- |
| 1 | admission と Safety が読む状態を確認 | CIログとstatsを先に取得 | op/arg/root/acceptedは入力記録専用 | 124を反例扱いしない既存試験を維持 |
| 2 | 入力記録だけを固定する射影を選択 | 修正前の全探索を計測 | applyWithTraceを両経路で共有 | 正モデルのstep引数を個別検査 |
| 3 | 入力選択肢とdispatch上限を維持 | 境界試験の失敗後に実装 | stepWithTraceを共有し選択を維持 | 負モデルにも同じstepを指定することを検査 |
| 4 | ITFの入力記録を維持 | disk正3設定と負例を先に確認 | TLC専用stepはregistryが所有 | 固定ITFを更新せず鮮度を検査 |
| 5 | 製品APIとguardに変更を加えない | 全registryとlintを確認してpush | 記録変数が遷移条件に使われないことを再読 | AAA・ケース分離を確認し実CIで再検証 |

software-design-philosophy の8診断は、この変更の検証runner・モデル境界で8/8、10/10。失敗項目なし。TLC用stepの選択はregistry、入力記録の省略はモデルが所有し、admission・状態更新・入力選択を複製していない。射影の根拠と適用範囲をREADMEに記載した。これは製品全体の正しさの採点ではない。

初稿の回帰試験2ケースは未対応のverification_step引数で失敗し、実装後に成功した。Quintの引数付きactionに戻り値型が必要だったため、最初のtypecheck失敗を修正した。いずれも意味的な負例検出には数えない。

disk の修正後は1242到達状態、4624ms（同じ20秒/RSS2048MiB制限）。正3設定・負例の意味的反例・固定ITFの鮮度に成功。関連Python試験85ケースも成功した。

全registryの16設定・10負例・102固定ITFも終了コード0で再検証した。Ruff check/format、ty、workflow lint、strict docs buildも成功した。Rustの製品コードと固定ITFには変更がないため、この修正ではRust全体の再ビルドを行わず、Linux CIの専用jobでも確認する。
