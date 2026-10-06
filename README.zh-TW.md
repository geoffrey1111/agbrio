<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">

# Agbrio

**Agent Bridge · 連接你的 Agent 對話**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

Agbrio 是面向開發者的個人桌面與手機協作工具。電腦保持執行，你可以在手機查看 Agent 的結果，挑選下一個對話需要的指令與資料，最後一次確認完成交接。

例如，一個 Codex 對話負責規劃與審查，另一個負責實作。Agbrio 保留兩端的明確身分，協助你把選定內容送回正確的原對話。

## 發布狀態

**正在準備開源發布。** 個人 Windows／Codex 實作已在使用，作者已回報手機轉發成功。目前公開儲存庫提供專案介紹、配圖與自行部署方案；**尚未發布應用程式原始碼、安裝程式、可用的安裝指令或選定的開源授權條款**。

## 電腦工作台，手機隨身查看

![Agbrio desktop reader](docs/assets/desktop-reader.png)

<p><img src="docs/assets/mobile-handoff.png" width="320" alt="Agbrio handoff selection with a selected instruction block"> <img src="docs/assets/mobile-conversation.png" width="320" alt="Agbrio original conversation and reply composer"></p>

配圖採用真實應用元件與虛構示例對話，由瀏覽器渲染。不含私人工作內容，也不代表 iPhone 或 Mac 實機驗收。應用控制項目前為中文；多語言 README 不代表應用已完成多語言支援。

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

歡迎在 [Issues](https://github.com/geoffrey1111/agbrio/issues) 提建議。回報時提供介面版本、裝置／系統、連線方式與重現步驟；公開前移除私人對話、路徑、權杖與敏感截圖。
