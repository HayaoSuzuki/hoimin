---
type: Decision
title: 資源制限・終了時の後処理・最終出力
description: OS別の資源制限と、プロセス終了・一時ファイル削除・レポート出力の完了条件を整理する。
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: issue-900-disk-failure
  resource: disk-failure.md
  working_tree: untracked
  sha256: 30398836956f8fd8687b718ba2c81e835125665d45e0e9ca3a470198b0f1eb60
- id: issue-867-shell
  resource: ../../../crates/hoimin-cli/src/shell.rs
  revision: 906ba668de94115ef5ca459af3b5f176d22b1b4e
  working_tree: modified
  sha256: 2713fa8220e276dd5e31e06dffde7828c1a61ecacd45786701adb91aae72a9ad
- id: issue-867-owned
  resource: ../../../crates/hoimin-cli/src/workspace/owned.rs
  revision: 906ba668de94115ef5ca459af3b5f176d22b1b4e
  working_tree: modified
  sha256: c2fb5ed580bf0ad852fbc43c42504c3cbd9cc418c673c3922b6b88b2c6697e33
- id: issue-858-process
  resource: ../../../crates/hoimin-cli/src/process/mod.rs
  revision: 6c638da243865273c55886739e9bbf77803f8fcd
  working_tree: modified
  sha256: d0ba6dbcdb372619e61909db4eb3fb08d90acfd6891b842fc23ca940d1a36498
- id: issue-844-shell
  resource: ../../../crates/hoimin-cli/src/shell.rs
  revision: 1713c3a17face763839ecd93793db52de8147f3f
  working_tree: modified
  sha256: 790f2890e17fa91a9da4d776e1d101dc7f2975f7f80e5f764df309494c98635a
- id: readme
  resource: ../../usage.md
  revision: 2831bde6adf75facf0a379b8a915267c278d8511
  working_tree: modified
  sha256: 873ffa0eb321441a2991cd1ea87e1d177c329128f24ad732ab0e81e6ce88a39a
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
- id: issue-488
  resource: ../../superpowers/specs/2026-09-11-issue-488-windows-resource-scope-design.md
  working_tree: untracked
  sha256: 9187c4029df0e3f1e0be45f43d11fa4b875201fcbcb7e0a22a4b4c0f31510d4a

- id: issue-487
  resource: ../../superpowers/specs/2026-09-11-issue-487-resource-report-policy-design.md
  working_tree: untracked
  sha256: 81204e5418b7fe3eac61795402199e9427a1b3157f89525d8267cb502d324e80

- id: issue-484
  resource: ../../superpowers/specs/2026-09-11-issue-484-metrics-destinations-design.md
  working_tree: untracked
  sha256: b146daf296d69374ad01ac86d5a5becd30b721b1983d5a54507cd0f1750ef3e7
- id: issue-872-operations
  resource: ../../../crates/hoimin-cli/src/workspace/operation.rs
  working_tree: untracked
  sha256: b71290b610725a6db3859f1fb47afb8550df81e4c85d138a24469c19810792c5

- id: issue-872-handler
  resource: ../../../crates/hoimin-cli/src/workspace/mod.rs
  revision: 49a34a63ac4e995bfefb156f85ff3a934b594e6b
  working_tree: modified
  sha256: ab7dd10ba3e60ee0a841115ff81ff36656572587e25dbb3ad19aa7968e91c173
- id: issue-889-lib
  resource: ../../../crates/hoimin-cli/src/lib.rs
  revision: 18634ef7fb0e5b4cd74ae959e019751af3465ed6
  working_tree: modified
  sha256: 34e86caeada4fd917bd334d151fce0efdadc150d351436a28edba38db7e8bd65

- id: issue-889-shell
  resource: ../../../crates/hoimin-cli/src/shell.rs
  revision: 18634ef7fb0e5b4cd74ae959e019751af3465ed6
  working_tree: modified
  sha256: 7eda76259a6a9533e0a6656ad7eb60ef2af74e59fc46647d6c9cde3cbc5a8926

- id: issue-889-delivery
  resource: ../../../crates/hoimin-cli/src/report/delivery.rs
  revision: 18634ef7fb0e5b4cd74ae959e019751af3465ed6
  working_tree: modified
  sha256: f0eb61d7bfd1bae452fe9993d8a058f902fc42c04d68a308dee995f14b52ca5f

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

利用方法の公開契約では、OSの機能でメモリとプロセス数の上限を強制する方式（hard enforcement）と、利用できる機能の範囲で制御する方式（best-effort）を区別している。WindowsではJob Object、Linuxでは権限を委譲されたcgroup v2が上限強制の基盤となる。これらを利用できない場合の条件は別に定められており、macOSではメモリ上限を強制しない。[^readme]

WindowsのJob ObjectとLinuxのcgroup v2では、baselineまたはmutantの各rootプロセスと子孫に、それぞれメモリ・プロセス数の上限全量を適用する。Linuxのプロセス数制限はスレッドを含むtask数である。並列度をJとすると合計は各上限のJ倍に近づきうる。Hoiminのrun用の親には合計メモリ・プロセス数の上限を設定せず、ホスト側の祖先の制限は引き続き有効である。resource modeのhardは制限の強制方法を表し、run全体の合計制限を意味しない。[^readme]

Unix向けの共通実装（portable経路）では、Hoiminが直接起動して管理するプロセス（root）が生存中にタイムアウトまたはキャンセルを受けると、そのプロセスグループを終了させ、rootの終了状態を回収する（reap）。rootが自然終了した後に残る子孫プロセスの後処理は保証範囲外である。これは利用方法に記載された契約の要約であり、今回すべてのOSで実測した結果ではない。[^readme]

# 最終結果の確定と出力完了

終了時の設計では、最終レポートとして出力する結果が選択済みになった後は、遅れて届いたキャンセル（cancel）や期限到達（deadline）によって、その結果を変更しない。後処理（cleanup）や最終出力を重ねて開始することもない。[^terminal]

ただし、結果を選択した時点では出力完了は確定していない。状態機械は、対応する出力完了通知（acknowledgement）を受け取って初めて `RunPhase::Finished` へ遷移する。[^terminal]

# レポート書込み中の所有権とディレクトリ削除

ディスク設計では、一時ディレクトリを用途ごとに分ける。実行用のディレクトリ（execution root）には、ソースのコピー、ワーカーの作業領域、プロセス出力、解析用の一時ファイルを置く。レポート出力用のディレクトリ（delivery root）には、最終出力まで保持するJSONレポートの一時データを置く。ここでのrootはディレクトリであり、前述のプロセスのrootとは対象が異なる。[^disk]

CLIとライブラリのrun/verify実行入口は、所有する `Write + Send + 'static` の書込み先（writer）を、完了まで呼出しが戻らない可能性のある処理（blocking操作）へ渡す。書込み完了の通知（acknowledge）は、実際の書込みが成功した後に行う。待機側がタイムアウトしても、blocking操作がwriterとdelivery rootを所有している間は、別の書込みや `flush` を開始せず、そのディレクトリも削除しない。タイムアウトは待機の終了を示すだけで、実行中のI/Oが停止した証拠にはならないためである。[^delivery][^issue-889-delivery]

一方、execution rootの削除可否は、プロセスと作業領域を使う処理が終了したかどうかで判断する。レポート出力が終わっていなくても、独立に安全条件を満たした実行用ディレクトリの後処理は進められる。借用writer向けの実行経路は廃止され、`run_with_writers` と `VerifiedPlan::execute` は所有writerを同じ配送経路へ渡す。短いprogress/review出力は、実行入口が所有するwriterへ同期的に書き込む。[^delivery][^issue-889-lib][^issue-889-shell]

書込み中に停止を観測した場合、停止要求を保持したまま同じ終了期限まで完了を待つ。期限内に完了した書込みの通知を先に受理し、その後で停止要求を適用する。完了通知を捨てると、終了処理が書込み済みの出力を再送する可能性がある。保持中の停止要求は新しい処理の開始を止め、期限後に観測した完了は出力完了として受理しない。[^issue-889-shell]

metricsは元ソースの検証後に書き込まれるため、出力先の衝突検査と最終書込みの許可を結び付ける必要がある。Issue484の設計ではbaseline前の検査結果を最終処理へ渡し、衝突時の書込みを防ぐ。通常のbaseline失敗時のmetricsと、書込み不能時の警告は維持する。[^issue-484]

保存先の同一性を確定できない場合は、実行結果を維持してmetricsの保存を見送り、終了時に `metrics.write` で通知する。未作成の親ディレクトリや対応範囲外の別名もこの扱いとし、既知の衝突をbaseline前に拒否する場合と区別する。[^issue-484]

# 後処理の要求・完了と監査の範囲

後処理を要求しただけでは、ディレクトリの物理削除やプロセスの終了回収（reap）が完了したことにはならない。削除を始める条件と、処理が完了したことを確認する条件を分けて扱う。[^disk][^cleanup]

資源管理の監査では、管理対象の登録情報（registry）を保護するロックと、対象ごとの後処理の所有権を扱っている。ロックを解放してOSの処理を待つ前に、後処理の所有権を確保する設計である。ディレクトリの物理削除と登録情報の更新は別の操作なので、削除済みのカウンターファイルを読み取り対象にしないための管理も必要になる。[^cleanup]

この監査の全ケースは、抽象モデル内だけで確認する `model-only` に分類されている。Rust側のテストは独立した内部テストであり、Leanが生成したテストケース群（corpus）と同じ前提・観測で照合する `strict` な対応検証ではない。このモデルの結果を、OSやRust実装全体の後処理の保証へ広げることはできない。[^cleanup]

# 歴史的設計と再確認する変更

ディスク設計には、当時の `tools/focused_mutation.py` とcargo-mutantsの連携も含まれる。このスクリプトはカタログの対象リビジョンには存在しないため、このカタログでは実行手順として案内しない。同設計の「最大4 jobs」はfocused cargo-mutantsの並列数であり、hoimin CLIの上限としては扱わない。[^disk][^readme]

[境界監査](../audits/boundary-2026-09.md)では、実行方式を表すresource modeの伝播や、Windowsで制限を実行全体（run-wide）へ適用する契約も調査対象になった。OSごとの制限実装（backend）、停止処理、所有権、レポート出力、制限を適用する単位を変更したら、公開説明と各OSで実行確認した範囲を再照合する。

# Windowsの制限単位と実機検証

Issue488では、Windowsのメモリ・プロセス数制限を、baselineまたはmutantのroot processとその子孫から成るtreeごとの上限として公開する。外側のrun用Job Objectは後処理を担い、内側の各Job ObjectがProcessLimitsを強制する。同時実行数を増やすと、合計使用量は設定した1回分の上限を超え得る。新たなrun全体の上限は実装しない。[^issue-488]

メモリはJob Objectが計上するcommit量、プロセス数はroot自身を含む数である。無視されていたconstructor引数を削除し、実際のrequest値とnative APIから読み出した上限を照合する。2つのtreeが同時に動く有限のbarrier試験で制限単位を確認し、違反したtreeへの帰属、正常なsibling、子孫の後処理も検証する。[^issue-488]

Windows Server2025の専用CIジョブでは、native unit25件、handler5件、実際のCLI2件がskipなしで成功した。各treeが96 MiBを同時に保持して合計192 MiBとなり、1回分の上限160 MiBを超えることを確認した。プロセス数も各2、合計4で1回分の上限3を超えた。これは当該fixtureの実測結果であり、すべての環境・実行順序を保証するものではない。macOS上の検査や、抽象モデルだけの性質は、Windowsが設定値を強制した証拠には含めない。[^issue-488]

# 実行方式の情報をレポートへ渡す

Issue487では、選択済みbackendのmodeと安定したmechanism名を、StartRequestedより前に状態機械へ渡す。すべての候補選択経路でこの情報を共有し、runヘッダーや停止時の未実行結果に架空のhard方式を設定しない。資源制限の実装や後処理を変える修正ではない。[^issue-487]

実際のbackend情報を渡したRustの遷移試験と、OSが上限を強制したという実機の証拠は区別する。Windowsの制限単位は独立したIssue488の対象であり、この情報伝播の修正だけでrun全体の上限を保証しない。[^issue-487]

[^readme]: [利用方法](../../usage.md)。
[^disk]: [2026-08-27-disk-safe-mutation-execution-design.md](../../superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md)。
[^terminal]: [2026-07-29-run-finished-terminal-design.md](../../superpowers/specs/2026-07-29-run-finished-terminal-design.md)。
[^delivery]: [2026-09-08-issue-335-report-shutdown-design.md](../../superpowers/specs/2026-09-08-issue-335-report-shutdown-design.md)。
[^cleanup]: [2026-09-08-issue-342-resource-audit.md](../../superpowers/reports/2026-09-08-issue-342-resource-audit.md)。

[^issue-488]: [2026-09-11-issue-488-windows-resource-scope-design.md](../../superpowers/specs/2026-09-11-issue-488-windows-resource-scope-design.md)。

[^issue-487]: [2026-09-11-issue-487-resource-report-policy-design.md](../../superpowers/specs/2026-09-11-issue-487-resource-report-policy-design.md)。

[^issue-484]: [2026-09-11-issue-484-metrics-destinations-design.md](../../superpowers/specs/2026-09-11-issue-484-metrics-destinations-design.md)。

# Shell coordination entry point (Issue #844)

The [shell module contract](../../../crates/hoimin-cli/src/shell.rs) maps the
current dispatch, completion-acceptance and finalization boundaries. Consult it
before changing cancellation, deadlines or output failure handling; it points
to the existing guide, native tests and model comparisons. `DiskLifecycle`
controls safety admission, while `CleanupProgress` records local attempts and
cannot authorize deletion. Task termination and buffered ownership recovery do
not by themselves acknowledge completion to core.[^issue-844-shell]

This is a documentation change verified against the implementation, not a new
shutdown guarantee or a fresh proof of Rust/OS behavior. Existing model and
platform evidence above retains its original scope.

[^issue-844-shell]: [Shell coordination contract](../../../crates/hoimin-cli/src/shell.rs).

# Process lifecycle API (Issue #858)

The [process module contract](../../../crates/hoimin-cli/src/process/mod.rs)
separates owned lifecycle tasks, cancellation requests, drain evidence and
backend release. Dropping the caller's waiting future detaches the owned task;
the runtime must remain alive for it to progress. Cancellation requests do not
acknowledge termination, reap or output collection.[^issue-858-process]

`drain_for_shutdown` observes tracked activity and then attempts backend close
within the remaining shutdown budget. Callers coordinate dispatch, in-flight
setup and cancellation; the observation does not prevent later work. Reap and
output-quiescence evidence remain distinct from output validity and secondary
close errors. A close timeout does not erase already-observed drain evidence
and does not preempt blocking cleanup.[^issue-858-process]

This documents the existing implementation and backend limits. It adds no
shutdown guarantee, OS coverage or formal proof; the shell contract above and
[deadline invariants](../../development.md#shutdown-deadline-invariants) retain
their coordination role.

[^issue-858-process]: [Process lifecycle contract](../../../crates/hoimin-cli/src/process/mod.rs).

# Cleanup classification and retry evidence (Issue #867)

The shell keeps Failed and Deferred cleanup variants through ordinary execution,
delivery and outer finalization. Recordless thread failures carry their typed
kind; an unverified remaining root still forces deferral. Marker work cannot
rename the root, so its failure may retain the known active path. Production
diagnostic codes and text, bounded details and primary/secondary errors retain
their existing meaning.[^issue-867-shell]

Workspace cleanup attaches internal coordinator-contention evidence for busy
process and file locks. The shell retries that evidence within the original
absolute deadline; diagnostic wording and detail truncation do not decide retry
eligibility. Live-child, deadline and integrity deferrals carry no contention
reason. This metadata adds no report field or state-machine transition, and the
janitor and output acknowledgement conditions remain unchanged.[^issue-867-owned][^issue-867-shell]

The [implementation and verification record](../../superpowers/plans/2026-10-14-issue-867.md)
separates behavioral regression failures, executed Windows checks and static
review. The existing Lean-model adapters do not prove every Rust/OS execution;
the Unix shutdown correspondence case is outside this Windows run.

[^issue-867-shell]: [Typed shell cleanup boundaries](../../../crates/hoimin-cli/src/shell.rs).
[^issue-867-owned]: [Owned workspace cleanup evidence](../../../crates/hoimin-cli/src/workspace/owned.rs).


# Shared workspace operation policy (Issue #872)

Synchronous handlers and owned tasks use one private create/reset policy.
Creation validates a grant before touching pending cleanup, drops cleaned
ownership before replacement, and retains copied-import-root rejection as
pending cleanup. Reset discards a failed worker and retains pending ownership
if discard also fails. Depth errors remain primary; other reset failures retain
the original error followed by the discard diagnostic.[^issue-872-operations]

The operation outcome carries either one active workspace or an error with
optional pending ownership. Prepare transfers ownership to an owned task;
execute returns surviving ownership in its completion; acceptance restores the
handler registries. These boundaries preserve parallel execution and the
existing duplicate-completion checks. Synchronous apply still uses its explicit
candidate argument; owned apply uses the request candidate.[^issue-872-handler]

The [implementation and verification record](../../superpowers/plans/2026-10-14-issue-872.md)
records characterization-first tests and the Windows validation scope. This
extraction changes no retry condition, reservation contract, state transition,
public signature, report or saved format. The existing Lean workspace adapters
remain comparison evidence; this work adds no Lean model or proof.

[^issue-872-operations]: [Shared create and reset policy](../../../crates/hoimin-cli/src/workspace/operation.rs).
[^issue-872-handler]: [Workspace ownership entry points](../../../crates/hoimin-cli/src/workspace/mod.rs).

[^issue-889-lib]: [Owned writer implementation](../../../crates/hoimin-cli/src/lib.rs).

[^issue-889-shell]: [Owned writer implementation](../../../crates/hoimin-cli/src/shell.rs).

[^issue-889-delivery]: [Owned writer implementation](../../../crates/hoimin-cli/src/report/delivery.rs).


## Typed disk stop evidence

The [disk failure contract](disk-failure.md) describes required reason payloads,
first-stop retention, complete secondary evidence equality, and the intentional
saved failure format change. Report projection keeps its existing byte-count and
message fields. This representation change does not add an OS cleanup guarantee.[^issue-900-disk-failure]

[^issue-900-disk-failure]: [Typed disk failure contract](disk-failure.md).
