# Agbrio

**電腦上的 Agent 繼續工作，你在手機上掌握進展。**

[![Download](https://img.shields.io/github/v/release/geoffrey1111/agbrio?label=download)](https://github.com/geoffrey1111/agbrio/releases/latest)
[![Build](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml/badge.svg?branch=main)](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml)
[![MIT](https://img.shields.io/github/license/geoffrey1111/agbrio)](LICENSE)
[![Windows](https://img.shields.io/badge/Windows-x64-0078D4)](https://github.com/geoffrey1111/agbrio/releases/latest)

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

查看 Codex、Claude Code 對話，傳送訊息跟進，接收新回覆與待處理通知。讓管理與執行分工，也能由一個管理對話協調多個執行對話。

![桌面對話總覽與執行狀態](docs/assets/zh-CN/conversations-current.png)

## 離開電腦，也能看進度、回訊息

一眼看到執行中、等待確認與子對話狀態。進入原對話查看成果、補充要求、回答支援的提問，不用遠端控制整個桌面。

<p align="center"><img src="docs/assets/zh-CN/mobile-conversations-current.png" width="300" alt="手機對話列表與子對話狀態"> &nbsp; <img src="docs/assets/zh-CN/mobile-reply-current.png" width="300" alt="在手機回覆原對話"></p>

## 選擇對話，開始監聽或綁定協作

從電腦上已有的對話中選擇。監聽單一對話，隨時跟進；把規劃與執行連成 Bridge，選擇轉交內容並確認對象，由你批准傳送。

<p align="center"><img src="docs/assets/zh-CN/select-conversation-current.png" width="300" alt="選擇電腦上的既有對話進行監聽"> &nbsp; <img src="docs/assets/zh-CN/review-current.png" width="300" alt="傳送前審閱內容與對象"></p>

![選擇 Bridge 的管理端與執行端](docs/assets/zh-CN/binding-current.png)

## 把日常跟進交給管理對話

授權一個管理對話協調多個 Codex 執行對話：讀取進展、傳遞任務、彙整結果；需要你決定時，再交回給你。管理對話可來自 Claude Code 或 Codex。

```mermaid
flowchart TD
  U["你 · 方向與關鍵決策"] <--> M["管理對話 · Claude Code / Codex"]
  M <--> A["Codex · 介面實作"]
  M <--> B["Codex · 介接開發"]
  M <--> C["Codex · 測試驗收"]
```

管理委派支援向 Codex 傳送；Claude Code 對話支援你本人從手機回覆。關鍵決策與授權範圍始終由你掌握。

## 開始使用

1. 下載 Windows 版，連接你正在使用的 Agent。
2. 選擇對話開始監聽，或綁定管理端與執行端。
3. 在「設定 → 裝置」配對手機，開啟你的 Agbrio 網址。

手機存取時，電腦與 Agbrio 需要保持上線；請在接收裝置啟用通知。

[Download](https://github.com/geoffrey1111/agbrio/releases/latest) · [使用說明](docs/START_HERE.md)

<details>
<summary>也有這些實用功能</summary>

直接閱讀對話中的文件、圖片與專案看板。需要自訂整合時，再使用 API / MCP。

</details>

<sub>截圖為目前介面，使用虛構示例資料。</sub>


## macOS 預覽候選（Apple Silicon）

macOS 適配仍在進行，尚未與 Windows 功能對齊。手動 macOS candidate DMG Actions 僅產生 macOS 13.5+ / Apple Silicon 的 ad-hoc 簽署 .dmg artifact，不建立 Release、不存放更新私鑰；macOS source 獨立檢查測試原始碼。實際執行成功前編譯打包仍未驗證。部分功能顯示「此平台尚未支援」。

核對校驗和後，將 Agbrio.app 放入 Applications。本人信任的未簽署/ad-hoc 候選若遭阻擋，可到系統設定 → 隱私權與安全性 → 仍要打開，或執行下方命令。應用程式不會自動解除 quarantine。候選不需要 Apple Developer 帳號，但沒有 Developer ID 簽署或公證。

```text
xattr -dr com.apple.quarantine /Applications/Agbrio.app
```

僅在實際功能需要時授予通知、所選檔案的檔案與檔案夾/卸除式卷宗權限，以及受保護位置所需的完整磁碟存取。未來恢復唯讀 AX 才需要輔助使用；舊瀏覽器路徑仍停用。Pi 提權、全本地卷宗授權、特權 helper 與簽署的應用程式內更新尚未實作。ad-hoc 身分隨建置改變，更新後可能需要重新授權。CI/打包/啟動通過不代表完整功能或使用者 UAT。
