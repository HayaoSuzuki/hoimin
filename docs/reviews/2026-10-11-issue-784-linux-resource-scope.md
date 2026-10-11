# Issue784 Linux資源制限の説明訂正

対象は [#784](https://github.com/HayaoSuzuki/hoimin/issues/784)。
基点は#783の `2831bde6adf75facf0a379b8a915267c278d8511`、ブランチは `docs/issue-784-linux-resource-scope`。
利用方法と知識カタログを実装の各root tree単位へ合わせる文書のみの変更。

## 設計のセルフレビュー

1. LinuxRunCgroup::probe_with_diagnosticsはRunLimitsを使用せずrun cgroupに合計上限を設定しないことを確認した。
2. prepare_rootがroot cgroupごとにmemory.maxとpids.maxを設定することを確認した。
3. machine.rsが各mutantのProcessLimitsへ設定値全量を渡すことを確認した。
4. [Linux kernelのcgroup v2文書](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid)でpids controllerはTIDを数え、スレッドも上限に含むことを確認した。メモリはcgroupに計上される量でありWindowsのcommitted memoryと混同しない。
5. 既存の契約を正しく説明する対応に絞った。合計上限の新設、配布物、CLI、既定値は変更しない。

## 実装計画のセルフレビュー

1. オプション表だけでなくOS説明・並列設定例・レポート理由の説明を点検対象にした。
2. WindowsとLinuxの共通の適用単位を示し、メモリ計上とtask数の違いは区別する。
3. コピー量とtotal timeoutのrun-wide契約は維持し、無差別な文字列置換はしない。
4. 知識カタログの要約も訂正し、根拠であるusage.mdのrevisionとsha256を更新する。
5. コードを変更しないため新規テストや資源制御の再実装実験は行わず、ソース照合・文書整合性・diff検証を完了条件にした。

## 実装のセルフレビュー

1. max-memoryとmax-processesの表からLinux run-wideという誤記を除いた。
2. 各baseline/mutant rootへの全量適用、J並列時の合計、親run cgroupと祖先の制限を本文で区別した。
3. best-effort、macOS、RLIMIT_AS、タイムアウトと終了回収の記述を維持した。
4. 並列例の「Linuxは共有」を訂正し、process_limit理由のrun-wideという誤記も直した。
5. 知識カタログへ現行適用単位を追記し、古い設計の歴史的説明は現行契約として書き換えなかった。

## テスト段階のセルフレビュー

1. 現行ソースのrun/root cgroup設定とmutantのProcessLimitsを再照合し、追加した説明に対応する実装を確認した。
2. 公開文書・知識カタログを検索し、Linuxのmemoryがshared across workersまたはrun-wideという現行案内が残っていないことを確認した。
3. コピー量・総時間など本来run-wideである説明は残っていることをdiffで確認した。
4. OKF38ページ、変更文書のローカルリンクとsource ID/脚注を検証した。既存sourceにrevision省略があるため検証補助スクリプトの参照だけをgetへ修正し、文書の制約を追加しなかった。usage.mdの新しいsha256は別途直接比較した。
5. git diff --checkで空白エラーがないことを確認した。文書の文言を固定する新規テストは追加せず、Hosted CIの結果も確認する。
