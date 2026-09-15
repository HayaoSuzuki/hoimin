# Issue #545 Implicit Finally Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline; user authorizes autonomous execution and reviews.

**Goal:** finally 注釈で暗黙例外前の非 typing 束縛を見落とさない。
**Architecture:** 暗黙例外を `ControlFlowExits` の独立状態として伝播し、finally 入口で交差を取る。
**Tech Stack:** Rust、Lean 4.32.2、JSON Lines、公開 CLI。
**Spec:** [設計](../specs/2026-09-15-issue-545-implicit-finally-design.md)

## Global Constraints

- 作業範囲は issue-545 worktree。
- import 自体の失敗と動的 hook はモデル外。
- 既存の正常経路と明示的 abrupt の分類を保持する。

## Task 1: 再現と期待値

Files: `formal/HoiminOracle/ImplicitFinallyAuditMain.lean`、`corpus/implicit-finally.jsonl`、`crates/hoimin-cli/tests/lean_implicit_finally_oracle.rs`。

- [x] Lean で `mayRaise` は現在状態を例外入口へ追加し、`importTyping` は正常状態を typing にする。
- [x] `allTyping [custom, typing] = false` と、例外入口を捨てる壊れたモデルとの不一致を証明する。
- [x] Lean で fixture を生成し、公開 `plan --operators type_list_sequence` の候補数・内容を比較する。
- [x] production 変更前に `cargo test -p hoimin-cli --test lean_implicit_finally_oracle` を実行して意味的不一致を記録する。

## Task 2: 実装

Files: `crates/hoimin-cli/src/analyzer/rust.rs`。

- [x] `implicit_raises: Option<KnownImports>` を `ControlFlowExits` に追加する。
- [x] 文の即時評価式に call/subscript/attribute または演算・比較がある場合、入口を clone して束縛変更を無効化する。子 suite の走査を止める。assert、反復、context manager、class 構築、handler 型式の入口も含める。
- [x] suite、branch、loop、try、finally へ暗黙例外を伝播する。finally 通常終了後は暗黙例外の分類を維持する。
- [x] Task 1 のテストと既存 `lean_nested_try_flow_oracle`、内部 oracle を実行する。

## Task 3: 文書と提出

- [x] `cargo fmt --all -- --check` と関連 analyzer テストを実行する。
- [x] OKF analyzer 概念、入口、設計書一覧、報告一覧を更新し、YAML とリンクと出典ハッシュを検証する。
- [x] 各工程で5回の異なる観点の自己レビューを記録する。
- [x] git diff を確認してコミットする。PR本文を作成し、親エージェントへ提出する。
