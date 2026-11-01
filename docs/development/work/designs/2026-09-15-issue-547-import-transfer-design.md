# Issue #547: 型解析の単一路でimport状態の所有権を移す

## 対象と問題

対象版は `f11013542ccd735ab9741b5079c0b39a517df256`。型注釈のcollectorは、既知のimport名を保持する `KnownImports` を文ごとに複製する。import数Iと注釈数Aを増やすと、候補がなくてもI×A個以上のentryをコピーする。[Issue #547](https://github.com/tokyogas-tech/hoimin/issues/547) が報告した問題である。

## 契約と採用案

単一路では状態を次の文へ移す。`ControlFlowExits.fallthrough`（次の文へ到達する状態）へ `std::mem::take` で移し、suiteは `Option::take` で受け取る。suite境界では現在のcollector状態を参照する既存呼出し元のために複製を残す。break・continue・return・raiseの状態、合流、関数・classの復元は既存の契約を維持する。

COW共有は分岐時の複製も減らせるが、書込み経路と計数規則の変更が広がる。永続mapの導入は依存追加になる。今回の単一路の問題は所有権移動だけで解消できるため、いずれも採用しない。各annotationのsnapshotを保存する方式へ戻さない。

## 検証と限界

`KnownImports::clone` にある実entry計数をcollector全体で観測する。IとAを独立に0/8/32/128へ増やし、通常のmodule import列と注釈列ではclone呼出し数が定数、entryコピーがI以下であることを要求する。型演算子未選択では0とする。実候補と分岐・loop・finally・到達不能文は既存のLean対応試験と追加fixtureで検査する。壁時計時間だけで合否を決めない。

これは単一路についての計数契約であり、分岐数や固定点反復数に対する全体の線形時間保証ではない。#546の入れ子ループの再走査、#545の暗黙例外の意味変更は別PRである。依存追加、CLIの公開形式変更、Rust 1.88のMSRV変更は行わない。
