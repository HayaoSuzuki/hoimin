# Quint 状態遷移検証の拡張

初期導入は `cad49400`。予算台帳2回の予約だけでなく、公開レポート境界と内部の完了台帳へ検証対象を広げる。Quint/TLC の成功、モデル履歴と実装の対応、広い生成入力を別の証跡として残す。

## 採用する構成

- BudgetExtended：各容量2、3種類、量0〜3を保ち、成功予約を4回まで探索する。解放後の再予約を含む既存 BudgetLedger の公開 API へ ITF を再生する。旧2予約モデルも維持する。
- Report：公開 `ReportSequence::validate/observe` に対応する開始・終了、2 mutant の順不同完了、ID と sequence の整合、単調なイベント番号、無効入力時の状態不変。有限 TLC は2 run ID、2 mutant ID、2 mutant sequence、イベント sequence 上限5を使う。長い履歴と u64 境界は PBT/fuzz の参照モデルで扱う。contracts 有効時は validate で先に入力を検査して無効 producer イベントの panic を避ける。
- Completion：本番 `RunState::allocate_id/register/accept_completion/retire_pending` の完了台帳へ内部 fixture から対応付ける。最大4要求、2種類の完了、任意順、重複、未知 ID、誤種別、汎用失敗、retire 後の遅延通知と新規要求を扱う。floor/gaps の圧縮形式を参照モデルに複製せず、完了・retired の集合から期待値を導く。公開 transition の全状態機械や OS cleanup の証明とは区別する。

TLA+ を別途手書きして遷移の知識を二重管理する案と、探索数だけを増やす案を比較した。Quint を唯一の有限モデルにし、異なる本番 API へ対応付ける構成を採る。プロセス間公平性、無限履歴の liveness、実装の ID 枯渇までをこの変更で証明するものではない。

## 知識の所有

`tools/verify_quint.py` がモデル登録、資源上限、正負の対照、fixture 鮮度を所有する。モデルごとに実行スクリプトを増やさない。共通 ITF reader は namespace と整数・集合の形式を所有し、個別 adapter は対象 API への対応だけを扱う。新たな製品用公開 API は追加しない。Report は公開境界の検証、Completion は内部 fixture の対応であることを文書とテスト名で示す。

TLC は順次・1 worker、heap 1 GiB、各コマンド20秒・RSS 2048 MiB。まず有限の小領域で成功を確認してから定めた上限へ拡大する。時間切れを成功にしたり、未完了の全探索を証明と呼んだりしない。各領域に意味的な不具合を入れたモデルと、実 Rust コピーへの mutation の双方で検出力を確認する。
