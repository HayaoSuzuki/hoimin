---
type: Decision
title: 資源制限・終了時の後処理・最終出力
description: OS別の資源制限と、プロセス終了・一時ファイル削除・レポート出力の完了条件を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: readme
  resource: ../../../README.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: disk
  resource: ../../superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: terminal
  resource: ../../superpowers/specs/2026-07-29-run-finished-terminal-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: delivery
  resource: ../../superpowers/specs/2026-09-08-issue-335-report-shutdown-design.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: cleanup
  resource: ../../superpowers/reports/2026-09-08-issue-342-resource-audit.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
---

# 制限する資源とディスク監視の範囲

Hoiminは、実行にかかった経過時間（wall-clock時間）、子孫プロセスのメモリ使用量とプロセス数、保持する出力の量、コピーするファイルの量、生成する作業領域の使用量、ファイルシステムの空き容量を別々に扱う。ディスクに関する既定値は次のとおりである。[^readme]

| オプション | 対象 | 既定値 |
| --- | --- | --- |
| `--max-copy-size` | 全ワーカーへコピーするソースファイルの合計量 | 1GiB |
| `--max-workspace-size` | 生成した作業領域とHoiminが所有する出力の使用量 | 8GiB |
| `--min-free-space` | 次の処理を始める前に確保するファイルシステムの空き容量 | 10GiB |

コピー量の上限では、テスト開始後に新しく書き込まれるファイルを制限できない。そのため、ディスク制限の設計では、Hoiminが所有する作業領域の使用量と、その領域を置いたファイルシステムの空き容量をそれぞれ監視する。[^readme][^disk]

この監視は一定間隔で測定する方式であり、全OSで合計使用量の上限を強制する仕組み（portableなhard aggregate quota）ではない。測定の間に子プロセスが空き領域を消費する可能性がある。終了時に作業領域を削除する際も、Hoiminが所有する領域であることと、その領域を使う処理の終了（quiescence）を確認する必要がある。ディスク設計では、プロセスの終了回収、標準出力・標準エラーの読取り完了、監視処理の終了確認を削除の前提としている。[^disk]

# OSごとの制限とプロセス終了の保証

READMEの公開契約では、OSの機能でメモリとプロセス数の上限を強制する方式（hard enforcement）と、利用できる機能の範囲で制御する方式（best-effort）を区別している。WindowsではJob Object、Linuxでは権限を委譲されたcgroup v2が上限強制の基盤となる。これらを利用できない場合の条件は別に定められており、macOSではメモリ上限を強制しない。[^readme]

Unix向けの共通実装（portable経路）では、Hoiminが直接起動して管理するプロセス（root）が生存中にタイムアウトまたはキャンセルを受けると、そのプロセスグループを終了させ、rootの終了状態を回収する（reap）。rootが自然終了した後に残る子孫プロセスの後処理は保証範囲外である。これはREADMEに記載された契約の要約であり、今回すべてのOSで実測した結果ではない。[^readme]

# 最終結果の確定と出力完了

終了時の設計では、最終レポートとして出力する結果が選択済みになった後は、遅れて届いたキャンセル（cancel）や期限到達（deadline）によって、その結果を変更しない。後処理（cleanup）や最終出力を重ねて開始することもない。[^terminal]

ただし、結果を選択した時点では出力完了は確定していない。状態機械は、対応する出力完了通知（acknowledgement）を受け取って初めて `RunPhase::Finished` へ遷移する。[^terminal]

# レポート書込み中の所有権とディレクトリ削除

ディスク設計では、一時ディレクトリを用途ごとに分ける。実行用のディレクトリ（execution root）には、ソースのコピー、ワーカーの作業領域、プロセス出力、解析用の一時ファイルを置く。レポート出力用のディレクトリ（delivery root）には、最終出力まで保持するJSONレポートの一時データを置く。ここでのrootはディレクトリであり、前述のプロセスのrootとは対象が異なる。[^disk]

レポート出力の後続設計では、CLIが所有する書込み先（writer）を、完了まで呼出しが戻らない可能性のある処理（blocking操作）へ渡す。書込み完了の通知（acknowledge）は、実際の書込みが成功した後に行う。待機側がタイムアウトしても、blocking操作がwriterとdelivery rootを所有している間は、別の書込みや `flush` を開始せず、そのディレクトリも削除しない。タイムアウトは待機の終了を示すだけで、実行中のI/Oが停止した証拠にはならないためである。[^delivery]

一方、execution rootの削除可否は、プロセスと作業領域を使う処理が終了したかどうかで判断する。レポート出力が終わっていなくても、独立に安全条件を満たした実行用ディレクトリの後処理は進められる。この出力方式の保証はCLIが所有するwriterを対象とし、呼出し元から借用した、別スレッドへ移せないwriter（非 `Send` writer）を受け取る同期APIにまで広げていない。[^delivery]

# 後処理の要求・完了と監査の範囲

後処理を要求しただけでは、ディレクトリの物理削除やプロセスの終了回収（reap）が完了したことにはならない。削除を始める条件と、処理が完了したことを確認する条件を分けて扱う。[^disk][^cleanup]

資源管理の監査では、管理対象の登録情報（registry）を保護するロックと、対象ごとの後処理の所有権を扱っている。ロックを解放してOSの処理を待つ前に、後処理の所有権を確保する設計である。ディレクトリの物理削除と登録情報の更新は別の操作なので、削除済みのカウンターファイルを読み取り対象にしないための管理も必要になる。[^cleanup]

この監査の全ケースは、抽象モデル内だけで確認する `model-only` に分類されている。Rust側のテストは独立した内部テストであり、Leanが生成したテストケース群（corpus）と同じ前提・観測で照合する `strict` な対応検証ではない。このモデルの結果を、OSやRust実装全体の後処理の保証へ広げることはできない。[^cleanup]

# 歴史的設計と再確認する変更

ディスク設計には、当時の `tools/focused_mutation.py` とcargo-mutantsの連携も含まれる。このスクリプトはカタログの対象リビジョンには存在しないため、このカタログでは実行手順として案内しない。同設計の「最大4 jobs」はfocused cargo-mutantsの並列数であり、hoimin CLIの上限としては扱わない。[^disk][^readme]

[境界監査](../audits/boundary-2026-09.md)では、実行方式を表すresource modeの伝播や、Windowsで制限を実行全体（run-wide）へ適用する契約も調査対象になった。OSごとの制限実装（backend）、停止処理、所有権、レポート出力、制限を適用する単位を変更したら、公開説明と各OSで実行確認した範囲を再照合する。

[^readme]: [README.md](../../../README.md)。
[^disk]: [2026-08-27-disk-safe-mutation-execution-design.md](../../superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md)。
[^terminal]: [2026-07-29-run-finished-terminal-design.md](../../superpowers/specs/2026-07-29-run-finished-terminal-design.md)。
[^delivery]: [2026-09-08-issue-335-report-shutdown-design.md](../../superpowers/specs/2026-09-08-issue-335-report-shutdown-design.md)。
[^cleanup]: [2026-09-08-issue-342-resource-audit.md](../../superpowers/reports/2026-09-08-issue-342-resource-audit.md)。
