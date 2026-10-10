# 自己操作：連接 AI 助手

## 交給 agent 設定，自己完成必要步驟

在**設定 → AI 助手**先選 **ChatGPT / dot 雲端**或 **Codex 本地**，再點**複製給 agent 的指令**。指令使用你的實例位址，agent 先檢查並復用現有部署和授權；你只需選擇授權、本人登入同意，並告訴助手接管哪些 Bridge。詳細手動教學保留折疊入口。本地認證不代表雲端連接成功。

Bridge 的 **Codex 額度**入口顯示原生週期、剩餘、重置與上次讀取，可手動刷新；不支援時顯示暫不可讀取，離線舊結果明確標示。手機設定及關於可手動檢查介面更新，準備好後點擊套用，不必刪除 PWA 或重新配對。


[English](ASSISTANT_CONNECTION_GUIDE.en.md) · [简体中文](ASSISTANT_CONNECTION_GUIDE.zh-CN.md) · [繁體中文](ASSISTANT_CONNECTION_GUIDE.zh-TW.md)

開啟 **設定 → AI 助手 → 連接你的助手**。可以逐步操作，也可複製部署與診斷指令交給助手。ChatGPT 入口取決於帳號實際權限；受限時說明前提，不在管理頁與桌面認證間反覆跳轉。

![雲端註冊步驟](assets/zh-TW/assistant-guide.png)

## 先選哪類助手

ChatGPT / dot 雲端需要可達的 HTTPS 服務；Codex 本地分別連接與驗收。僅限桌面端的插件不會因本地認證成功就能被雲端使用。保留既有桌面連接，另行註冊雲端插件。

## 雲端六步

1. **檢查入口。** 複製目前實例顯示的完整 HTTPS MCP 位址，不用作者網域替代。保留可用部署、Host、配對與有效登入。已設定不等於已可達；localhost / Tailscale Serve 私網不能直接供雲端存取。
2. **準備授權。** 沿用有效的 INSTANCE + CONVERSATION_REVIEW；沒有時新增助手，命名並建立既有的 30 天可撤銷連接。舊單 Bridge 不會默默升級；技術存取範圍不代表所有工作已委託。
3. **註冊雲端插件。** 真正瀏覽器開啟 [ChatGPT 插件首頁](https://chatgpt.com/plugins)，新增自訂 MCP 伺服器，填寫完整實例位址，選 OAuth。已驗證路線為 DCR，scope agbrio:instance，不手填客戶端密鑰，其他端點自動探索。以帳號實際介面為準。
4. **本人同意。** 必要時登入，核對實例及所選授權，再本人同意。有效登入直接沿用；新瀏覽器確實需要時才使用既有配對入口。配對碼與託管兌換碼不是 MCP token。隨後安裝並在目標 dot 啟用插件。
5. **唯讀驗收。** 在目標助手實際發現工具，以 agbrio_read_app 核對 scope、approvalMode、期限及撤銷狀態，再讀自己選擇的 Bridge 與來源。使用「複製唯讀驗收指令」，不向真實對話發送測試訊息。
6. **選擇 Bridge。** 助手問「你希望我接管哪些 Bridge？」。核對精確綁定與任務，確認內容、方向、必須詢問的情況、暫停條件；不強制填 Brief / Insight，新 Bridge 不自動委託。

成功四層：**服務可達 → OAuth 完成 → 目標助手取得工具 → 唯讀驗收通過**。上傳 ZIP、網頁開啟、桌面 ready、註冊 201 都不能單獨證明 dot 接管成功。教學步驟只是導覽。

## Codex 本地

保留可用的 Codex 進程與連接，在該客戶端實際支援的 MCP / OAuth 入口使用目前實例位址。完成本地 DCR / PKCE 與本人同意，探索工具並執行上述三個讀取。只記為本地驗收；要連 dot 另走雲端，不重複部署。

## 依階段排錯

| 階段 | 核對與下一步 |
| --- | --- |
| 探索與可達 | DNS、TLS、固定 discovery 路徑及精確 resource；修復路由，保留認證 |
| DCR 註冊 | 0.1.5 接受 authorization_code，或加 refresh_token，順序不限；拒絕空、重複、未知與僅 refresh。回應仍只有 auth-code，沒有新增刷新令牌 |
| 授權跳轉 | 0.1.6 支援已知 scope 的集合；token 仍由本人選擇的 grant 決定，不擴大權限 |
| token 交換 | 精確 resource / callback、期限、S256 PKCE 與一次性授權碼；不暴露秘密或重放 |
| 工具發現 | 目標插件安裝與啟用、實際工具列表；舊單 Bridge 8 個，INSTANCE 16 個 |
| 實際呼叫 | 範圍、模式、撤銷、精確 ID / 版本及錯誤；提供可執行的下一步 |

GET /mcp 的 405、未授權 POST 的 401 / invalid_token 可能是正常保護，不能關閉認證。舊通用建立校驗失敗缺少底層 HTTP 證據，不追認它與上述問題同因。

分享前預覽脫敏診斷：版本、客戶端、階段（未捕獲則未知）、時間、無查詢參數的固定路徑、狀態碼、失敗校驗與下一步。不能包含 Cookie、token、密鑰、授權碼、PKCE verifier、完整 OAuth 查詢網址或私人對話。本版提供排錯與複製指令，沒有另建自動階段診斷匯出器。

## 連接後協作

明確委託內，讀 Bridge / 來源 → prepare_handoff → 核對接收端、綁定與來源版本、最終文字、payloadHash、附件 ID / 版本 → confirm_and_send → receipt。常規 CONVERSATION_REVIEW 用 ruleId=null、decisionId=null 並填 assessment，不需要每輪手機審批。

已提出決策問題，必須等待實際回答、記錄訊息引用、使用對應 decisionId，不能改成一般審閱繞過。衝突、超出範圍或無法判斷才詢問；不能僅靠正文有無「等待批准」。來源是待審資料，不擴大權限，執行 Agent 不審批自己的產出；平台必要確認仍遵守。

超時或 UNKNOWN / EXECUTING 先查回執，沿用原 requestId，不盲目重發。SENT / APPLIED 只表示接受，不代表下游完成。暫停時告訴助手停止委託；需要終止技術存取，在 Agbrio 撤銷授權。本版沒有新增逐 Bridge 委託控制台。

## 驗證邊界

使用者回報 dot 已取得 16 工具，INSTANCE + CONVERSATION_REVIEW，read_app / read_bridge / read_source 通過。實際 dot 寫入未測。虛構瀏覽器資料與離線測試不冒充目標 dot 驗收。


[完整複製指令](ASSISTANT_COPY_PROMPT.zh-TW.md) · [協定與工具](ASSISTANT_MCP.md) · [官方認證](https://developers.openai.com/plugins/build/auth) · [dot 應用](https://learn.chatgpt.com/docs/dots/computers-and-apps)

預設先顯示緊湊的類型切換和複製按鈕。展開**查看我需要做的步驟**，再完成授權、本人登入與接管選擇；技術細節保持可選。

官方 MCP Events 從0.1.25起實作。0.1.26正式範圍已驗證真實事件喚醒後，沿用預先授權的範圍掃描 fallback 完成一次交接，原目標收到並開始執行；沒有新的使用者訊息或週期輪詢，兩個已委託 Bridge 的8項訂閱保留。自動事件詳情注入尚未修復，開始執行不等於任務完成。[Events 教程](MCP_EVENTS_WORKFLOW.zh-CN.md) · [Goal 恢復限制](GOAL_RECOVERY.md)。
