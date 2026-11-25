# SQLite セッション検証のレビュー記録

基点 `241be31c`、作業ブランチ `test/quint-state-machines`。
設計・計画の各5回レビューは [設計](design.md) と [計画](plan.md) に記録した。
以下は作者のセルフレビューであり、独立した第三者のコードレビューではない。

## 実装の5回レビュー

- I1: Lifecycle と Selection を別モデルに分離し、候補選択・結果置換・所有権の直積を避けた。TLC はそれぞれ3174状態、335状態を全探索した。
- I2: Quint の予約語と List/Set の演算を点検し、present/create と indices().forall を使用した。typecheck と TLC の両方を通す。
- I3: モデルの owned を DB 行だけからは観測できない。各操作後に公開 lookup を二つの mutant に実行し、所有権と再利用可否を検査するよう補った。
- I4: 期待する拒否を is_err のみで判定すると I/O 障害を成功扱いしうる。許可した semantic error code のみを受理し、その他は試験失敗とした。begin は同じコードを I/O エラーにも使うため、operation と UNIQUE constraint の理由まで照合する。
- I5: DB 要求、SQL 観測、再オープン、snapshot の知識を一つの driver に集約した。製品 API に試験用引数を追加せず、CI も既存 runner を再利用する。

software-design-philosophy の最終診断は8/8、10/10（追加した検証のモジュール境界に限定）。失敗項目なし。公開入口と内部検証の区別、情報隠蔽、責務、コメント、レビュー、変更の局所化、知識の所有、設計改善を確認した。製品全体の証明の点数ではない。

## テストの5回レビュー

- T1: テスト関数は1履歴1ケースとし、AAA を明記した。反復と分岐は driver に置き、literal/ITF は rstest で個別ケース化した。
- T2: 旧結果7種類と置換7種類を49ケースに展開した。Killed/Survived の同値再保存も拒否されることを検査する。
- T3: 拒否時の結果 status 比較だけでは付随行の破損を見逃す。5テーブルの全行 snapshot を比較し、診断の不一致で置換 transaction を失敗させた後にも再オープンと lookup を行う。
- T4: PBT の予算を小整数だけでなく255/256とusize::MAXまで広げ、big-endian BLOB の境界を扱う。実 DB を入力ごとに作る fuzzer は追加せず、64ケースの有限 PBT と既存の純粋状態機械 fuzz を使う。
- T5: 正しいモデルだけで完了とせず、負例モデルと隔離コピーの製品変異、通常/全features、既存全体試験、lint、文書生成を実行し、実測値を下記へ記録する。

## 実行結果

全11設定・8負例・59履歴の鮮度検査が成功。追加モデルの全探索は Session が3174状態、SessionSelection が335状態だった。再検証時の guard 実時間と peak RSS はそれぞれ5.071秒・約644MiB、3.820秒・約455MiBで、各20秒・2048MiBの制限内。

追加73ケースは通常構成と全features構成で成功。3種類の PBT はそれぞれ64ケース。workspace 全targets/allfeatures Clippy、fmt、ruff check/format、ty、workflow lint、strict docs build が成功。

隔離コピーで6種類の製品変異を実行し、すべて assertion の失敗で検出した。compile error は検出に数えていない。

| 変異 | 失敗ケース数 |
| --- | ---: |
| 確定済み結果を未確定扱いして上書きする | 16 |
| 最新ではなく最古の適合 run を選ぶ | 3 |
| 完了済み run を再開候補に含める | 2 |
| load 成功時に保存予算を更新しない | 6 |
| 診断不一致でも置換済み transaction を commit する | 6 |
| Killed を再利用せず再実行する | 14 |

既存 disk_lifecycle libFuzzer を pinned nightly で15秒指定して実行し、16秒・283435入力で異常なし。seed1172、入力timeout5秒、RSS1024MiB、max_len4096。生成 corpus は seed と分離した。

Python 全体の初回実行ではプロセス一覧を取得する guard が sandbox 制限で126を返し、7件失敗した。これは検出成功として扱わず、必要な実行権限で全体を再実行した。Python 全体の再実行は2780成功・35スキップ（107.71秒）。Rust 全体は `RUST_TEST_THREADS=4 cargo test --locked --workspace --no-fail-fast` で244 suite、18861成功・失敗0・62 ignored。最後に begin のエラー分類を補強した driver を通常構成・全features構成で再実行し、各73ケースが成功した。workspace Clippy も再実行して成功。
