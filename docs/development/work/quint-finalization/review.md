# 終了・削除許可・所有権のレビュー記録

2026-11-25、起点 `9c31d188`、作業ブランチ `test/quint-state-machines`。
[設計](design.md) と [実装計画](plan.md) に従って3領域を追加した。
software-design-philosophy の8診断で責務・入口・情報隠蔽・契約・レビュー・知識の所有・境界説明・設計改善を確認した。以下は作者による各工程5回のレビューである。

## 設計の5回レビュー

| 回 | 検討 | 判断・修正 |
| --- | --- | --- |
| D1 | カウンタ上限の拡大だけでは別の危険な遷移を扱えない。 | 公開 DiskLifecycle、CLI 削除許可と evidence projection、実 OS RunOwnership の3領域へ接続する。 |
| D2 | OS 削除や SQLite recovery まで証明したように扱っていないか。 | Finalization は実メソッドの内部対応、Ownership は別 open の file handle と probe。削除・SQLite transaction・ホスト障害の証明とは区別する。 |
| D3 | Disk の2 dispatch workload が20秒制限に達した。TLC の出力に完了があっても外側124は成功でない。 | 上限は延ばさず1 dispatch に縮小。長い複数 dispatch は PBT/fuzz で扱う。 |
| D4 | cleanup の failed/retained と delivery の clean 必須を混同していないか。 | 両ルート、execution のみ、ルートなしを別設定で全探索。failed drain を retained で閉じられる設定も検証。 |
| D5 | テスト用 API と protocol decoder が増殖していないか。 | 新しい製品 API を作らず、CLI の ITF reader を1箇所に集約。構成の最終診断8/8、10/10。失敗診断は残っていない。 |

点数は追加したモジュールの境界と知識の所有に対する判断であり、OS や製品全体の正しさの点数ではない。

## 計画の5回レビュー

| 回 | 確認 | 判断・修正 |
| --- | --- | --- |
| P1 | どの本番メソッドへ接続するか。 | Disk は公開 apply/snapshot、Finalization は実際の inner/outer drains・measurement・cleanup・available query、Ownership は実 OS acquire/drop。 |
| P2 | 有限モデルと生成性質の役割。 | ITF の状態・結果を直接照合し、PBT は拒否の atomicity、失敗の固定、query の一回性、descriptor 排他などの法則を確認。 |
| P3 | 正常入力だけに偏っていないか。 | clean/deferred、refused/unjoined/available、reacquire の固定入力 schedule を同じモデル action から生成。 |
| P4 | リソースと CI の失敗伝播。 | 各20秒・RSS2048MiB・heap1GiB・worker1を継承。CLI内部検査と実 OS 検査も CI に追加。 |
| P5 | 本番実装が誤ってもモデル内だけで成功しないか。 | 隔離コピーへの6種類の Rust mutation、通常/全features、全suite、実 fuzz を計画した。入口と証拠区分を含む最終診断8/8、10/10。 |

## 実装の5回レビュー

| 回 | 発見・確認 | 修正・結果 |
| --- | --- | --- |
| I1 | Quint の boolean 代入で or の結合が action と解釈された。 | RHS を括弧で囲み typecheck。全モデル・正負検査・再生成一致を実行。 |
| I2 | CLI 内で2回読み込んだ ITF module が duplicate_mod になった。 | lib の test-only module に集約し、個別 adapter は alias を使う。snapshot の符号化も既存 helper を再利用。 |
| I3 | PBT の期待する safe/attempted/cleaned を本番 getter から取ると、同じ欠落を見逃す。 | 生の状態から独立に判定し、getter 自身も比較。joined IO error の evidence 保持と failed query がゼロにならない法則も追加。 |
| I4 | 11 fuzz targets の起動 reserve は121秒で、既存の正常系 fixture の120秒では成立しなかった。 | 3テストが期待外の setup budget failure となった。fixture を180秒にし、実 campaign の deadline・reserve 条件は変更せず再確認。 |
| I5 | shared decoder の境界、native handle の破棄順、infra error の分類を再確認。 | handle を先に drop してから temp directory を削除。OS IO error は busy に数えず失敗。構成の最終診断8/8、10/10。 |

## テストの5回レビュー

| 回 | 確認 | 判断・結果 |
| --- | --- | --- |
| T1 | 1テスト1履歴、AAA、parametrization。 | テスト本体の反復・分岐は driver/strategy へ分離。literal と ITF を rstest の個別ケースで実行。 |
| T2 | 有限上限の外。 | disk/finalization は256ケース・最大127操作と任意 u64、ownership は64ケース・最大63操作で4 actor/3 run ID。0とMAXを欠測と区別する。 |
| T3 | 実 OS とモデルの相違。 | 各操作後に独立した open で全 run を probe。同 actor の再 open も競合し、他 actor の release は既存 owner の権利を壊さないことを確認。 |
| T4 | 期待する失敗を正しく数えているか。 | mutation 初稿のコピーが source の src/target を build 出力と誤って除外した compile error は検出成功に数えず修正。実 assertion の失敗で再確認した。 |
| T5 | 回帰・資源・文書。 | 新規46ケース、全features、全suite、clippy/fmt/ruff/ty/workflow lint、strict docs build、正負のTLCと全48履歴の鮮度を確認する。最終結果は下記。 |

## モデル検査

`python tools/verify_quint.py` が9設定・6負の対照・48履歴の再生成一致を成功させた。
新規5設定の到達状態数は次のとおり。既存4設定は67932状態であり、設定間で重なる状態があるため、合計を互いに異なる実装状態の数と扱わない。

| 設定 | 到達状態 | guard 実時間 | peak RSS |
| --- | ---: | ---: | ---: |
| DiskLifecycle | 25546 | 15.116秒 | 約738 MiB |
| DiskExecutionOnly | 11110 | 8.565秒 | 約716 MiB |
| DiskNoRoots | 5986 | 6.483秒 | 約714 MiB |
| Ownership | 69 | 3.733秒 | 約418 MiB |
| Finalization | 14753 | 5.092秒 | 約625 MiB |

モデルの finite workload・順序に対する安全性検査であり、任意の OS interleaving や無限履歴の liveness を証明するものではない。Source-only の Ownership モデル提案を参考にしたが、その agent による checkout 読取・実行・独立コードレビューは行われていない。

## 実装 mutation

通常成功すべきテストが assertion で失敗することを、一時コピーで確認した。作業ツリーの本番実装は変更していない。

| 壊した条件 | 検出 |
| --- | --- |
| Deferred delivery を finish 可能にする | 指定 ITF と literal/PBT |
| failed safety 後の clean/failed cleanup を許す | literal/PBT |
| 後からの計測成功で unjoined を修復する | ITF・literal・PBT |
| 削除許可から measurement quiescence を外す | ITF・literal・独立した生状態の PBT |
| failed free-space query を再試行する | ITF・literal・PBT |
| OS の try_lock を呼ばず ownership を返す | OS probe・ITF・literal・PBT、9件すべて失敗 |

## 最終実行

新規の通常/全features検査46ケースが成功。disk_lifecycle libFuzzer は15秒指定・497007実行（16秒）、異常なし。入力の timeout5秒・RSS1024MiB、最大128操作。生成 corpus は seed と分離した。

Python 全体は2780成功・35スキップ（148.87秒）。Rust 全体は `RUST_TEST_THREADS=4 cargo test --locked --workspace --no-fail-fast` により243 suite、18788成功・失敗0・62 ignored。workspace clippy（全targets/features）、fmt、ruff、ty、workflow lint、strict docs build が成功。

追加 fuzz target の lint では pinned nightly に Clippy component がなかったため、その試行は実行できなかった。fuzz の実行自体は pinned nightly で成功している。

追加 target は repository 指定の stable toolchain による `cargo clippy --locked --manifest-path fuzz/Cargo.toml --bin disk_lifecycle --all-features -- -D warnings` でも成功した。
