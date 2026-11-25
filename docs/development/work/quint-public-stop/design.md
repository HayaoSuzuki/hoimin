# 公開停止プロトコルのモデル検証

RunState::new と公開 transition の正常な前段操作で初期状態を構築し、停止から cleanup、最終レポート、セッション終了までを検証する。内部 ledger の単体検証を、公開 API の効果・完了通知の順序へ拡張する。

初期地点はターゲット解決待ち、preflight 待ち、コピー予算確保後の開始レポート待ち、およびセッション開始後の開始レポート待ち。前二者は同じ有限抽象状態を共有する。開始済み worker と実プロセスの終了は対象に含めない。

停止原因はキャンセル・期限切れ・ディスク停止。通常 ack、generic failure、出力 I/O failure、retired/duplicate/unknown/wrong-kind completion、cleanup 予約不一致を入力とする。有限モデルでは停止を2回までに制限し、PBT と fuzz で長い履歴と任意 u64 のディスク観測値へ拡張する。

Quint の期待値と実装の公開観測（phase、exit code、pending、effect kind、output sequence、disk summary、RunFinished/FinishSession）を比較する。モデル内の補助フラグを実装の非公開状態と同一視しない。拒否された transition は state を返さないため、driver は clone した入力を渡し、エラー後は保持していた前状態から次の入力を試す。これは製品 API のエラー後 atomicity の証明ではない。

候補は内部 fixture の拡充、既存 machine テストだけの追加、公開 API と有限モデルの接続の三つ。今回は新たな実装境界を扱える三番目を選ぶ。実行基盤・ITF reader は既存のものを使い、試験専用の製品 API を追加しない。

## 設計レビュー5回

- D1: 前段も公開 API で構築し、内部 fixture の正しさを公開 protocol と取り違えない。
- D2: copy/session の有無を3設定に分離し、同じ停止前抽象状態を2つの公開 prefix に再生する。
- D3: 最終出力中・cleanup 中の停止は受理されるが状態を変えない。エラー拒否と区別する。
- D4: cleanup failure 後の diagnostic 中に再停止すると cleanup を再発行しうる。無条件の「cleanup は1回だけ」を不変条件にしない。
- D5: 補助状態と公開観測、clone による replay 方針と製品のエラー契約を区別する。

software-design-philosophy: 境界の診断8/8、10/10。失敗項目なし。prefix 構築、通知生成、公開観測の知識を一つの driver に集約する。

## 生成試験で発見した修正

開始出力が未確認のまま cleanup へ進み、最終出力にも失敗すると、開始出力を再発行する分岐が選ばれた。最終レポートの失敗による終了判定を「開始未確認」の再試行判定より先に評価する。公開 API の形は変更しない。回帰を修正前に失敗させ、同じ履歴の成功を修正後に確認する。
