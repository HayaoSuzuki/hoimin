---
type: Audit
title: 暗黙例外から finally へ入る束縛
description: Issue #545 の小モデル、公開 plan 対応、既存制御フローの検証範囲。
status: draft
catalog_revision: f11013542ccd735ab9741b5079c0b39a517df256
sources:
- id: design
  resource: ../../superpowers/specs/2026-09-15-issue-545-implicit-finally-design.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: f2d56cddd7659add3ef30fed812ac8174bf45e725fa0ef380f2ca39c8b0dda51
- id: review
  resource: ../../superpowers/reports/2026-09-15-issue-545-implicit-finally-review.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 04a697f4a1c037c5759f8d92b3185cace24cfa2775645d3035750ef649b483a3
- id: implementation
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: modified
  sha256: 8159fe087096a5d01bf392bdcd85f80ab8e8616a3a888be1767aa1a0a2427a10
- id: model
  resource: ../../../formal/HoiminOracle/ImplicitFinallyAuditMain.lean
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 0235a1a9ffbd5e0ef7c86d55b93f778bfef9e9d1ede4e470fa0a281c0a0dabfd
---

# 対象と契約

型注釈の名前が typing の import に由来するかを判定する。通常式が失敗して import を飛ばした入口でも同じ束縛である場合だけ、finally 内で型の変異候補を許可する。[^design][^implementation]

# 証拠の範囲

Lean の3イベントモデルは、custom 束縛で始まる `mayRaise` に任意の後続列を足しても、全入口が typing になるとは判定できないことを証明する。入口を捨てる壊れたモデルとの相違もカーネルで確認する。6件の生成 fixture は call/subscript/attribute の負例と import 先行・明示 raise・通常 import の対照を含む。[^model]

公開 plan の比較は候補数、原文、置換、バイト範囲を検査する。別の Rust テストは入れ子、else/handler/loop、遅延実行、正常後続、walrus など14入力と、演算・assert・反復・context manager・class・handler 型式の11入力、名前・コンテナ・書式化・部分束縛の9入力、match guard 前のcaptureの1入力を確認する。実行結果と再現コマンドは報告書に記録する。[^review]

# 限界と再確認条件

このモデルは import 自体の失敗、全 Python 構文、任意の動的 hook を証明しない。演算や class 構築は保守的に例外が起き得ると扱い、実際の値に基づく無例外性は証明しない。正常候補の抑制範囲、式 visitor、finally の伝播、関数の遅延評価境界を変えた際はモデルと実装の対応を再確認する。[^design][^review]

[^design]: [設計](../../superpowers/specs/2026-09-15-issue-545-implicit-finally-design.md)。
[^review]: [検証記録](../../superpowers/reports/2026-09-15-issue-545-implicit-finally-review.md)。
[^implementation]: [解析器](../../../crates/hoimin-cli/src/analyzer/rust.rs)。
[^model]: [Lean モデルと生成器](../../../formal/HoiminOracle/ImplicitFinallyAuditMain.lean)。
