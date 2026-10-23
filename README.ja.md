# Agbrio

**Agent の作業は PC で。進捗の確認と次の指示はスマートフォンから。**

[![Download](https://img.shields.io/github/v/release/geoffrey1111/agbrio?label=download)](https://github.com/geoffrey1111/agbrio/releases/latest)
[![Build](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml/badge.svg?branch=main)](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml)
[![MIT](https://img.shields.io/github/license/geoffrey1111/agbrio)](LICENSE)
[![Windows](https://img.shields.io/badge/Windows-x64-0078D4)](https://github.com/geoffrey1111/agbrio/releases/latest)

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

Codex と Claude Code の会話を確認し、返信して作業を進める。新しい結果や判断が必要なときは通知を受け取れます。計画と実行を分担したり、ひとつの管理会話に複数の実行会話を任せたりできます。

![Desktop conversation overview](docs/assets/en/conversations-current.png)

## 外出先でも、進捗を見て返信する

実行中・確認待ち・子会話の状態を一覧で把握。元の会話で結果を読み、追加の指示や対応している質問への回答ができます。

<p align="center"><img src="docs/assets/en/mobile-conversations-current.png" width="300" alt="Phone conversation list and activity"> &nbsp; <img src="docs/assets/en/mobile-reply-current.png" width="300" alt="Reply to the original conversation from your phone"></p>

## 会話を選んで、監視や連携を始める

PC 上の既存の会話を選択。ひとつの会話を追跡することも、計画役と実行役を Bridge でつなぐこともできます。転送する内容と相手を確認し、あなたが承認します。

<p align="center"><img src="docs/assets/en/select-conversation-current.png" width="300" alt="Choose a conversation to watch"> &nbsp; <img src="docs/assets/en/review-current.png" width="300" alt="Review content and recipient before sending"></p>

![Choose a planning conversation and executor](docs/assets/en/binding-current.png)

## 日々の確認を管理会話に任せる

ひとつの管理会話が複数の Codex 会話の進捗を読み、タスクを伝え、結果をまとめます。判断が必要な場面はあなたへ。管理側には Claude Code または Codex を使えます。

```mermaid
flowchart TD
  U["あなた · 方針と重要な判断"] <--> M["管理会話 · Claude Code / Codex"]
  M <--> A["Codex · 画面実装"]
  M <--> B["Codex · API 開発"]
  M <--> C["Codex · 検証"]
```

委任したメッセージの送信先は Codex です。Claude Code にはスマートフォンから本人が返信できます。判断と委任範囲はあなたが管理します。

## 使い始める

1. Windows 版をダウンロードし、使用中の Agent を接続。
2. 会話を選んで追跡するか、管理役と実行役を接続。
3. 設定 → デバイスでスマートフォンをペアリングし、Agbrio の URL を開く。

PC と Agbrio はオンラインにしておいてください。受信端末で通知を有効にしてください。

[Download](https://github.com/geoffrey1111/agbrio/releases/latest) · [セットアップ](docs/START_HERE.md)

<details>
<summary>そのほかの便利な機能</summary>

会話内の文書・画像・プロジェクトボードも閲覧できます。独自の連携には API / MCP を利用できます。

</details>

<sub>画像は現在の画面を使った架空のデモデータです。</sub>
