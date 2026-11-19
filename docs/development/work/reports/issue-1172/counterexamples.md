# #1172 の反例と境界条件

本表は仕様上の不明点と、仕様に反する実装を想定した反例を区別する。`reduce` は未実装なので、いずれも本番実装の confirmed bug ではない。corpus の mode はすべて `model-only`。

C1–C3 は初回監査の未定義箇所の記録であり、現在は [確定した補足](clarifications.md) によって解消している。

## C1 — 保持指定と入れ子の除去

- **claim:** 保持した定義の ID を retained に残せば、保持指定を守れる。
- **model boundary:** ID と、元ソースに対応する正しい半開区間を入力として受ける。
- **minimal input:** outer の削除範囲 `[0,30)`、保持する inner の範囲 `[20,30)`。inner の ID は retained に残っている。
- **intermediate states:** ID 検査は成功するが、外側の範囲の除去で inner のコードも消える。
- **classification:** specification ambiguity。Q10 の保持優先を意味として守るには、ID だけでなく実際の編集範囲も検査する必要がある。
- **implementation correspondence:** 未実装。AST の範囲と actual diff の対応は将来の adapter で確認する。
- **accepted resolution:** 保持した定義を内包・交差する除去も禁止する。モデルはこの解釈を採用する。重なった削除範囲の正規化方法は editor の設計事項であり、この証明は扱わない。
- **reproduction:** `generate_reduce_spec -- --sensitivity` の `retention/ancestor-erases-keep`。corpus の `ancestor-erases-keep`。

## C2 — 短縮率とゼロ時間の未定義境界

- **claim:** `d * reduced ≤ (d - n) * original` だけで実行時間の短縮を保証できる。
- **model boundary:** 自然数 tick と正確な有理数 `n/d`。浮動小数点や CLI の小数入力形式は含まない。
- **minimal input:** `n=0,d=20,original=1,reduced=1` は式を満たすが、短縮していない。`n=d=20,original=1,reduced=0` も式を満たす。
- **intermediate states:** 明示されていない入力領域を許すと、等速やゼロ tick が採用条件を通る。
- **classification:** specification ambiguity。Q18 は利用者による率の変更を認めるが、変更可能範囲とゼロ時間の扱いを決めていない。
- **implementation correspondence:** 未実装。
- **accepted resolution:** `0 < n < d`、全時間観測 `>0` を要求する。ゼロ tick は認証しない。モデルと現在の `agreed` 行はこの領域制約を使う。領域外は設定エラー、観測済みゼロ時間は測定不成立として停止する。自動再測定はしない。未観測の枠を観測されたゼロと混同しない。
- **reproduction:** sensitivity の `boundary/zero-rate-equal-time` と `boundary/hundred-percent-zero-time`。`pairPass_strict` は上記の追加前提を含む述語について証明する。

## C3 — 締切と完了が同時の場合

- **claim:** 締切と同じ時刻に完了した候補を publish してよいかが一意に決まる。
- **model boundary:** 一つの単調論理時計で、証跡全体の完成時刻を観測する。OS の kill 応答時間や JSON/diff の物理書込み時間の保証ではない。
- **minimal input:** deadline `1`、証明条件を満たす候補の完成 tick `1`。
- **intermediate states:** completion 優先なら publish、deadline 優先なら停止して新規 publish しない。
- **classification:** specification ambiguity。Q20/Q22 は予算と未確認案禁止を定めるが、同時境界の優先順位は明示しない。
- **implementation correspondence:** 未実装。
- **accepted resolution:** `completedAt < deadline` を要求し、同時なら deadline 優先。すでに認証済みの案を失う必要はない。モデルと trace 行はこの確定した補足を採用する。
- **reproduction:** sensitivity の `precedence/deadline-equality`、定理 `deadline_no_publish`。

## R1 — 個別削除可能性は同時削除可能性を意味しない

- **claim:** A、B をそれぞれ単独で外せるなら、両方を外せる。
- **minimal input:** ミュータント一つを A、B の両方が kill。片方の削除では kill、両方の削除では survived。
- **classification:** 仕様に反する探索実装の反例。Q15 は採用後の集合を基準に再評価すると定めており、仕様の矛盾ではない。
- **implementation correspondence:** 未実装。オラクルの組合せケースは評価結果を入力とする。実際の非加法的な runner を用いた対応試験が別途必要。
- **owner decision:** 既存合意どおり、実際の組合せの検証結果を要求する。

## R2 — 丸め、中央値、結果件数だけの比較

- **minimal inputs:** (a) `original=1,reduced=1,n/d=1/20` で割引量を先に切り捨てると等速を許す。(b) final pairs `100→90,100→90,100→101` は中央値では合格しても一組が退行。(c) 同じ件数で異なる mutant ID、または重複 ID の結果。
- **classification:** 仕様に反する実装の反例。Q17/Q18 の契約は成立している。
- **implementation correspondence:** 未実装。自然数のクロス積による評価を生成するので、将来の固定幅整数での overflow を成功扱いにしないことが adapter の義務。
- **reproduction:** sensitivity の `arithmetic/rounded-discount`、`timing/median-hides-pair`、`uniqueness/duplicate-results`。corpus の `same-count-different-ids`。

## R3 — 証跡の誤用と早すぎる publish

- **minimal inputs:** final pairs が二組だけの候補を完成扱いにする、別 variant の証明結果を流用する、削減後 baseline から timeout を再計算する。
- **classification:** 仕様に反する実装の反例。Q19/Q22/Q23 の証跡契約を実装時に強制する必要がある。
- **implementation correspondence:** 未実装。
- **reproduction:** sensitivity の `atomicity/publish-before-complete`、`identity/reused-variant-certificate`、`provenance/recomputed-timeout`。

## M1 — 監査モデル自体の修正

初稿の区間判定は端点の交差だけを比較し、空区間 `[1,1)` と `[0,2)` を重なりと判定していた。これは仕様の欠陥ではなく model defect。Lean の失敗例で確認後、両区間の非空条件を追加した。`empty_span_no_overlap` と `overlap_symmetric`、全端点 `0..3` の生成ケースで保持する。

## Infrastructure ledger

最初の resource guard は sandbox 内でプロセス監視を実行できず exit 126 (`monitor_error`) となった。意味上の反例には数えていない。承認付きで同じ guard を実行して以後の測定を行った。Lean の構文・証明・ビルド登録エラーもモデル開発中の失敗であり、本番不一致ではない。

既存ライブラリ全体の `lake build +HoiminOracle:o` は、プロセスツリー RSS 約 3025 MiB で 2048 MiB の上限を超え、1.7 秒で guard が停止した。子プロセスが残っていないことを確認し、上限を引き上げて再試行していない。全体ルートへの不要な追加 import は外し、対象モジュールと外部 consumer の検証に限定した。全体ビルドは未検証として残す。CI は既存の直列 module build に新しい対象を登録する。
