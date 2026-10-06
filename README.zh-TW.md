<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">

# Agbrio

**Agent Bridge · 讓管理對話與執行對話順暢交接**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

如果你已經習慣讓一個 Agent 對話管理專案，另一個對話實際執行，Agbrio 就是圍繞這種用法設計的。

管理對話保留規劃、審查結果、決定下一項任務；執行對話在專案中實作修改，回傳結果與證據。**Bridge 負責把選定的指令、結果與資料在這兩個既有對話之間交接，每次傳送都由你確認。**

手機讓你隨時審閱與確認交接。產品的重點是兩個對話之間的橋梁，遠端存取是到達工作台的方式。

## 發布狀態

**正在準備開源發布。** 個人 Windows／Codex 實作已在使用，作者已回報手機轉發成功。目前公開儲存庫提供專案介紹、配圖與自行部署方案；**尚未發布應用程式原始碼、安裝程式、可用的安裝指令或選定的開源授權條款**。

## 適合已經使用雙對話協作的人

這是為主動分開管理與執行上下文的人準備的工作流程工具。兩個對話與角色由你選擇。Agbrio 不自動分配 Agent，也不讓管理 Agent 自行核准自己的結果。

**指令 → 執行端；結果與證據 → 管理端；最終確認 → 由你完成。**

## 一個對話管理，一個對話執行，Bridge 連接交接過程

![Managing and executing conversations side by side in one Bridge](docs/assets/desktop-bridge.png)

<p><img src="docs/assets/mobile-handoff.png" width="320" alt="Management-to-execution instruction selection"> <img src="docs/assets/mobile-return.png" width="320" alt="Execution-to-management result selection"></p>

電腦圖展示同一個 Bridge 的雙端對照；兩張手機圖分別展示「管理端向執行端下發指令」和「執行端向管理端交回結果」的選擇介面。採用真實元件、虛構資料與瀏覽器渲染，不含私人對話，也不代表裝置驗收。應用控制項目前為中文，多語言 README 不代表應用已完成多語言支援。

## 個人版本目前能做什麼

- **Bridge 雙端協作** — 綁定既有的控制端與執行端對話。查看任一端，並將執行結果交回控制端。
- **選擇內容後交接** — 閱讀完整原文，整塊勾選段落、修改傳送內容、確認接收端，最後一次確認傳送。指令區塊識別只是建議，傳送什麼由你決定。
- **帶齊資料** — 選取的 Codex 交接檔案會複製到接收端專案的 `.aiwr/incoming/<handoff-id>/`，訊息附上相對路徑與 SHA-256。接收端讀取實際的本機副本；這不等於上傳成 ChatGPT 對話附件。
- **通知與原對話回覆** — 開啟特定結果通知，回覆對應的原對話。公開的使用者訊息與 Agent 回覆由舊到新排列，更早的訊息在上方載入。
- **桌面 Host ＋手機 PWA** — 電腦保存綁定、觀察結果與傳送紀錄，手機使用配對後的網頁工作階段。即時讀取與傳送需要電腦開機、保持喚醒並連網。

## 一次完整交接

1. 建立 Bridge，綁定準確的控制端與執行端對話。
2. 閱讀控制端回覆，選擇指令區塊與所需資料。
3. 核對接收端，按一次「確認傳送」。
4. 執行端回傳結果後，審閱並選擇內容，再交回控制端。

## 手機如何連接電腦

![Agbrio conceptual network and handoff diagram](docs/assets/connection-flow.svg)

概念結構：手機操作電腦 Host，Host 連接兩端 Agent 對話。「HTTPS 入口」代表使用者選擇的網路服務，不是 Agbrio 託管的帳號服務。

目前個人環境透過 Cloudflare Tunnel 提供 HTTPS 入口。Cloudflare 負責網路路徑，應用狀態與請求仍由電腦 Host 處理。PWA 可以先顯示快取介面，但快取內容不能證明電腦目前在線。

後續自行部署流程計畫允許使用者選擇自己的連線方式。以下是規劃選項，目前應用尚未提供可用的通用選擇器。

- **Cloudflare Tunnel** — 適合已有 Cloudflare 託管網域的使用者。
- **Tailscale Funnel** — 計畫使用服務商網域提供公開 HTTPS 入口，無須購買自己的網域。
- **Tailscale Serve** — 計畫提供私人網路入口，兩端都需要使用 Tailscale。
- **自己的 HTTPS 代理或隧道** — 計畫供已有遠端存取設定的使用者選用。

[自行部署設計與 Agent 設定指令草稿（簡體中文）](docs/SELF_HOSTING_PLAN.zh-CN.md)

## 確認、資料與送達

- 以準確的 conversation／thread 識別碼綁定目標，不依標題、截圖或介面位置猜測。
- 每次交接都需要使用者明確確認，不建立自動循環的管理端／執行端流程。
- 送達不確定時保留該狀態，先核對紀錄，不盲目重複傳送。
- 選定檔案的實際位元組與雜湊才是資料依據；只顯示來源電腦路徑，不會讓遠端接收端自動取得檔案。
- 自行部署仍需信任選用的隧道／網路服務商與 Agent 服務商；手機快取與本機保留訊息也屬私人資料。

## 目前邊界

目前個人使用路徑是 Windows ＋ Codex。ChatGPT 整合仍屬實驗功能，需要另行驗證帳號工作階段與完整收發流程。Claude Code、其他 Agent、macOS 與通用自行部署流程均為後續工作，並非已發布的支援承諾。iPhone PWA 的版面與渲染問題仍在排查。目前不提供託管的多使用者服務。

## 可用開源版本發布前

- 匯出乾淨的原始碼，審查相依套件與散布條件後決定授權條款。
- 實作連線方式選擇、連線驗證與可交給 Agent 的部署指令。
- 在全新 Windows 環境驗證安裝、配對、復原與檔案交接；Mac 獨立驗證。
- 透過獨立介接器擴充其他 Agent，分別驗證實際讀取與傳送。

## 聯絡作者與回饋

[作者頁面](https://github.com/geoffrey1111) · [回報問題](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml) · [提出工作流程建議](https://github.com/geoffrey1111/agbrio/issues/new?template=feature_request.yml)

使用問題也可以在 Issues 中交流。這是公開管道，請使用示例資料，並於發布前移除私人內容。
