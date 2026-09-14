# Issue #491: 入力形状ごとの性能回帰検証

## 対象と境界

https://github.com/tokyogas-tech/hoimin/issues/491 を対象とする。基準は `165a2d284a1af92eb02ffd214ba8c0070c2f3808`。対象発見、fingerprint、ソース配置、AST構造、解析状態、verify、出力、workspaceの8次元を一つの台帳へ登録する。各修正のアルゴリズムは個別PRで扱い、このPRから他PRをmainへマージしない。

## 採用案

JSON台帳に入力次元、独立に増やす量、N/2N/4N、期待する増加、測定指標、実行可能な回帰テストと依存Issueを登録する。通常のPRゲートはactiveなRustテストを実際に実行し、指定したテストが0件なら失敗する。未マージの修正に属するテストはpendingとして、成功件数へ含めない。依存PRの統合後にactiveへ昇格する条件を記載する。

別laneのPython計測器は、隔離した一時projectを作り、baselineと変更後のrelease CLIを同じfixtureで複数回実行する。既存のLean用resource guardを再利用し、単一CLIのdeadline・プロセスツリーRSS・子プロセス残留を監視する。これはLinux/macOSのPOSIX laneであり、Windowsは未実施として扱う。stdout/stderrをファイルに出し、PIPEの容量で停止しないようにする。

入力サイズ、候補数、truncated、終了コード、経過時間、サンプリングRSS、sample間隔、CLIのSHA-256、OS/CPU/Python/Rust、反復、中央値をJSONへ保存する。RSSが0の短時間プロセスは測定不能としてnullにし、0メモリ消費と扱わない。retained heap、allocator peak、RSSは台帳で区別する。時間/RSSに固定の合否閾値は置かず、正常な結果・上限・監視エラーは必ず検査する。

## 入力と対照

対象発見は選択ファイル一定で無関係なファイル数を増やす。fingerprintはファイル数一定で重複glob数を増やす。ソース配置は同じ候補を長い1行と複数行へ配置する。ASTは未選択のcollection内に選択演算子を置く。解析状態はimport幅×annotation数と再代入×呼出数を増やし、候補0件の対照を含める。verifyは同じ候補集合からtop1/topN、出力はrecord数、workspaceは無関係なコピー対象bytesを増やす。すべて有限のサイズ上限と外部deadlineを持つ。

## Leanとの対応

既存の `BoundedCandidateDiscoveryModel/Proofs/Cases` とその生成corpus・Rust adapterを再利用する。証明対象は候補prefix・順序・overflowと対象読取りであり、CLI全体の定数メモリ、native stack容量、経過秒数やRSSを結論しない。台帳からモデル・corpus・strict adapter・感度/freshnessコマンドへ参照できるようにする。新しい最適化の計数は個別PRが提供し、モデルの前提とproductionの観測点を対応付けて昇格する。

## 代替案と判断

すべての性能修正を一つのブランチへ取り込む案はIssue単位PRという要求に反する。時間だけの閾値は環境変動を失敗と誤認する。active/pendingを区別した共通台帳と、決定的な通常テスト・別lane計測を採用する。

## 設計セルフレビュー

1. Issueの8次元を照合し、候補保持数だけでは検出できない入力を登録する構造にした。
2. 依存PRが未マージなのにテストが存在すると報告する問題を避け、pendingを成功件数へ含めない契約を追加した。
3. guardのRSSはサンプリング値であること、短命なCLIを観測できない場合、時間測定の環境依存性を改訂版で明記した。
