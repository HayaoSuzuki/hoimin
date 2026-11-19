# Issue #1172: reduce の実装設計

## 契約

確定仕様は ../reports/issue-1172/specification-current.md と同ディレクトリの clarifications.md。保存 plan、明示 mutant IDs、ファイルと定義パターン、全体予算を受け取り、元ソースを書き換えずに Python 定義の除去案を探索する。目的関数は通常テスト3回の中央値。固定 mutant の結果を完全保存し、元と案を交互に3組測り、全組が正確な最低短縮率を満たした案だけから diff を作る。

## 比較した構成

既存 verify を subprocess として呼ぶ方式は、各測定で不要な discovery と mutant 実行が発生し、外側の終了と cleanup の所有権も増える。既存 shell に編集オプションを渡す方式は通常 run の状態機械へ reduce 固有の条件を持ち込む。採用する構成は、plan の既存検証を入口で使い、WorkspaceHandler と ProcessHandler の資源・コピー・終了処理を直接再利用する専用 session である。これは今回の実装判断であり、新たな製品仕様ではない。

## 知識の所有権

- hoimin-python の定義編集が AST、decorator、元 encoding とバイト位置、修飾名の重複、保持範囲、重複編集、空 suite の pass 補完を所有する。元 snapshot から案を生成し、呼出側は位置を計算しない。
- reduce の採用ポリシーが率、正の測定、完全な結果集合、identity、認証済み best と締切を所有する。Lean corpus の adapter は期待値を再実装しない。
- CLI session が検証済み plan、固定 timeout、逐次 worker、通常実行・mutant 実行、分割探索、予算予約、出力証跡を所有する。未認証の探索候補と認証済み best は別に保持する。

## 入力と成果物

`hoimin reduce PLAN --candidate ID ... --test PATH:QUALNAME_GLOB ... --keep PATH:QUALNAME_GLOB ... --total-timeout DURATION --min-reduction PERCENT --output DIR`。名前 glob は `*` と `?`、ファイルは正確な root 相対パス。率は十進百分率、既定5、厳密に0より大きく100未満。mutation scope は plan の明示 selector を引き継ぐ。source / fingerprint と編集ファイルが重なる入力は拒否する。

新規出力ディレクトリに report.json、実行ごとの保持出力、認証済み案がある場合だけ proposal.diff を保存する。JSON は選択・保持・候補、source/plan/command/environment の識別、凍結 timeout、各実行の時間・終了・出力参照、探索履歴、停止理由、最良案を含む。diff は確認した同じバイト列を再現する。通常の非ゼロ終了では assertion と収集エラーを区別できないこと、将来の不具合検出能力を保証しないことを記す。

## 探索・予算・停止

まとめた除去を先に試し、成立しない集合を二分する。採用後は残った集合を基準に再評価する。構文・通常実行失敗、kill 差は候補棄却。元 suite の kill がゼロなら探索しない。最終確認用の時間を予約して探索を終了し、総予算は延長しない。単調時計の完成時刻が締切以上なら認証しない。ゼロ時間と基盤・資源障害は停止、停止前に認証した案は保持する。cleanup と成果物配送は探索再開とは分離する。

## 検証

Lean の952行を production decision helper の internal-fixture と比較し、public 実行の証明とは区別する。編集、任意コマンド、固定 timeout、終了、予算、成果物は別の実装試験で確認する。rstest の各展開を1ケースとし、AAA、生成性質、fuzzing を使う。テスト本体の手書きループと条件分岐を避ける。

## 実行資源と終了契約の補足

各試行の前に immutable snapshot へ worker を復元する。前試行の生成物、pyc、fixture 副作用を次へ持ち越さない。元ソースの freshness は探索前後に既存の検査で確認する。既存 disk monitor を探索全体へ接続し、子実行は残り全体時間と個別timeoutの小さい方で制限する。キャンセル後は ProcessCancellation、drain、worker cleanup の順に所有権を終了する。

終了は、探索完了（案の有無を問わず）0、入力不正・元baseline失敗2、基盤障害3、予算・資源による未完了4、割込み130。最終確認には直近の元と候補の最大時間の3組分に20%の余裕を加えて予約する。余裕は予算延長ではない。最初の候補も最終確認を完了できる残時間がなければ評価しない。

成果物は存在しない新規ディレクトリだけへ作り、既存の最終要素（symlinkを含む）と元project内の指定を拒否する。親はcanonicalizeし、OS標準のsymlinkを含む親も受理する。失敗時にも既存入力を上書きしない。diff は元 encoding のバイト列で生成し、表現不能なケースを認証済みと報告しない。

## 実装後の所有権

成果物は保存先のdirectory capabilityから排他的に作成する。実行中はbackground monitor、実行とroot cleanup終了後は保持directoryの各公開の事前・事後測定で容量を検査する。最終公開でも追加bytesとfree-space reserveを検査し、書込み失敗時には部分ファイルを除去する。プロセスと出力のdrain証拠を得られなければworkerを削除せず、janitorへ保持を引き継ぐ。

入力のfull copy manifestと、編集対象のbounded no-follow読込みは公開前の条件である。終端の通常ファイルだけを無制限に読み直す実装は採用しない。確認用予約不足による打切りも、理由付きの未完了4として記録する。独立レビューと各段階5回のセルフレビューは ../reviews/2026-11-19-issue-1172-reduce.md に記録した。
