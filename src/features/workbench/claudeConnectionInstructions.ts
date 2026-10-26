import type {Language} from "../../i18n";
import {assistantMcpConfig} from "./assistantClientInstructions";

export function claudeCodeAddCommand(url:string|null):string|null{
 if(!assistantMcpConfig(url))return null;
 return `claude mcp add --transport http --scope user agbrio '${url!.replaceAll("'","''")}'`;
}

export function originalChatWorkflow(language:Language):string{
 if(language==="en")return `After read-only acceptance, handle only the owner's delegated task. CREATE_CHAT returns an empty registered Codex conversation, not a first task. To send the authorized first task, copy result.nextActionInput, add text, then agbrio_prepare_action(SEND_CHAT) → review the exact input/payloadHash → agbrio_execute_action under the actual grant approvalMode → agbrio_action_receipt. No prior notification or manually pasted first message is required. For existing chats read agbrio_read_chat.sendContext. sourceSequence is optional: omit it or use null for direct/first messages; never use 0, generation, turnId or a guessed message index. WATCH_EVENT_NOT_FOUND is a notification reference error, not proof that OAuth is broken. Inspect the exact receipt; correct and prepare again only when non-delivery is established, retaining the original receipt reference. UNKNOWN/SENDING/EXECUTING must never trigger a blind resend. Do not create a test conversation or send a test task merely to validate connection.`;
 if(language==="zh-TW")return `唯讀驗收後，只處理本人明確委託的任務。CREATE_CHAT 回傳已註冊的空 Codex 對話，不會自動發首個任務。傳送已授權的首個任務：複製 result.nextActionInput、補入 text，再 agbrio_prepare_action(SEND_CHAT) → 審閱精確 input/payloadHash → 按真實 grant.approvalMode 呼叫 agbrio_execute_action → agbrio_action_receipt。不需要先產生通知，也不要求本人先手動貼第一條消息。既有對話先讀 agbrio_read_chat.sendContext。sourceSequence 可省略或填 null；不可把 0、generation、turnId 或猜測的消息序號填進去。WATCH_EVENT_NOT_FOUND 是通知引用錯誤，不證明 OAuth 壞了。先查精確回執，證實未送達後才能修正參數重新準備，保留原回執引用；UNKNOWN/SENDING/EXECUTING 不盲目重送。不為連接驗收建立測試對話或發測試任務。`;
 return `只读验收后，只处理本人明确委托的任务。CREATE_CHAT 返回已注册的空 Codex 对话，不会自动发首个任务。发送已授权的首个任务：复制 result.nextActionInput、补入 text，再 agbrio_prepare_action(SEND_CHAT) → 审阅精确 input/payloadHash → 按真实 grant.approvalMode 调用 agbrio_execute_action → agbrio_action_receipt。不需要先产生通知，也不要求本人先手动贴第一条消息。已有对话先读 agbrio_read_chat.sendContext。sourceSequence 可省略或填 null；不可把 0、generation、turnId 或猜测的消息序号填进去。WATCH_EVENT_NOT_FOUND 是通知引用错误，不证明 OAuth 坏了。先查精确回执，证实未送达后才能修正参数重新准备，保留原回执引用；UNKNOWN/SENDING/EXECUTING 不盲目重发。不为连接验收创建测试对话或发测试任务。`;
}

export function claudeCodeSetupInstructions(url:string|null,language:Language):string{
 const command=claudeCodeAddCommand(url);
 const endpoint=url??"<MCP_URL_FROM_AGBRIO_SETTINGS>";
 const add=command??"<CONFIGURE_HTTPS_MCP_ENTRANCE_FIRST>";
 if(language==="en")return `Connect this exact Claude Code session to Agbrio. MCP endpoint: ${endpoint}.
1. Identify the actual client: terminal, VS Code, Desktop Code tab, or cloud. If unclear, ask once. A connection in another client is not evidence for this session. Check actual tools and existing configuration before changing anything. If agbrio_read_app is already available, call it first; on success reuse this proven connection and skip adding or signing in again. Do not claim installation or OAuth from previous messages, a URL, or a saved grant.
2. Terminal/VS Code: inspect claude --version, claude mcp list and claude mcp get agbrio. Reuse a matching server. If missing, add it in PowerShell: ${add}. Do not run a placeholder. An Added response means configuration was saved, not authenticated. Never remove another server or overwrite a different endpoint silently. If claude is unavailable, report that exact limitation and give the owner this one command for the machine that runs Claude Code.
3. Open /mcp in an interactive Claude Code session, select agbrio and sign in when required. The owner completes browser OAuth and consent. If no browser opens, use the login URL offered by that client locally; do not request tokens, codes, callback URLs, or credential files in chat. A valid existing login may be reused without a new popup; prove it with the actual tool call. Headless sessions cannot complete interactive login: provide the owner the exact /mcp step and wait for confirmation.
4. Desktop Code: use + beside the prompt → Add connectors (or Connectors → Browse connectors), or Settings → Connectors to add the current custom MCP endpoint and authorize it. Verify tools in the intended Code session; terminal configuration or another account does not prove Desktop acceptance. Do not direct this route to ChatGPT Plugins. Official references: https://code.claude.com/docs/en/mcp and https://code.claude.com/docs/en/desktop#connect-external-tools.
5. Discover agbrio_read_app in this session and call it read-only. Report actual scope, approvalMode, expiresAt/revokedAt, and one readable result. Only this proves the connection. An HTTP response, saved configuration or an assistant saying connected is insufficient. If tools are missing, inspect /mcp status and refresh this client's tools through its supported controls; do not interrupt running tasks. Report the exact failed layer: missing CLI/server, transport, OAuth, tool discovery or application arguments, and continue repair until the read succeeds or a specific owner action is required.
An MCP server is not necessarily an installed plugin: look in /mcp or Connectors, not only the plugin browser. The separate Agbrio Claude bridge plugin lets Agbrio observe/reply to Claude sessions and deliver supported notices; managing Codex through MCP does not require that bridge plugin. If the owner also wants to monitor Claude in Agbrio, use Agbrio's Claude connection flow separately; do not claim both features installed from one pasted prompt.
Do not send test messages, create subscriptions or start tasks during connection acceptance. After the actual read succeeds, ask what the owner wants managed and preserve existing approvals and at-most-once receipts.`;
 if(language==="zh-TW")return `請把當前這個 Claude Code 對話連接到 Agbrio。MCP 位址：${endpoint}。
1. 先辨認終端、VS Code、桌面 Code 分頁或雲端會話；不清楚就問一次。其他客戶端連上不代表這個會話連上。檢查實際工具與既有設定；若已有 agbrio_read_app，先實際呼叫，成功就復用這條已驗證連接，不重複新增或登入。不能根據舊對話、網址或已建立授權宣稱安裝/OAuth 成功。
2. 終端/VS Code：實際檢查 claude --version、claude mcp list、claude mcp get agbrio。同位址可復用；缺少才在 PowerShell 新增：${add}。佔位符不能執行。Added 只代表設定已儲存，不代表已認證；不能靜默覆蓋不同位址或刪其他伺服器。沒有 claude 指令時，說明實際限制，把這一條指令交給使用者在執行 Claude Code 的電腦操作。
3. 在可互動的 Claude Code 輸入 /mcp，選 agbrio，按需要登入。本人完成瀏覽器 OAuth 與同意；沒有開瀏覽器時，在本機開啟客戶端提供的登入連結，不索取 token、驗證碼、回呼網址或憑據檔。有效舊登入可以不再彈窗，但仍要實際呼叫工具證明。非互動模式不能完成登入，須明確請本人完成 /mcp 這一步並等待回覆。
4. 桌面 Code：輸入框旁 + → Add connectors（或 Connectors → Browse connectors），或 Settings → Connectors，新增當前自訂 MCP 位址並授權；在目標 Code 對話驗證工具，不把終端或其他帳號成功當成桌面驗收。不走 ChatGPT 插件入口。官方說明：https://code.claude.com/docs/en/mcp 與 https://code.claude.com/docs/en/desktop#connect-external-tools。
5. 在當前會話真正發現 agbrio_read_app 並唯讀呼叫，回報實際 scope、approvalMode、expiresAt/revokedAt 和一項讀取結果，才能說連接完成。HTTP 有回應、設定存在或助手自稱已連上都不足夠。缺少工具時檢查 /mcp 狀態，透過客戶端支援的操作更新工具，不打斷執行中的任務。按缺少 CLI/伺服器、傳輸、OAuth、工具發現、業務參數分層排查，繼續修復直到讀取成功或明確需要本人的哪一步。
MCP 伺服器不一定出現在插件列表，應查看 /mcp 或連接器。Agbrio 的 Claude 橋接插件是另一項功能，用來讓 Agbrio 監聽/回覆 Claude 對話與遞送支援的通知；用 MCP 管理 Codex 不要求先裝橋接插件。若也要在 Agbrio 監聽 Claude，另走 Agbrio 的 Claude 連接流程，不能僅貼提示詞就宣稱兩邊都已安裝。
只讀驗收不發測試消息、不建訂閱、不啟動任務。實際讀取通過後再問本人要管理什麼，保留既有審閱與回執規則。`;
 return `请把当前这个 Claude Code 对话连接到 Agbrio。MCP 地址：${endpoint}。
1. 先辨认终端、VS Code、桌面 Code 页签还是云端会话；不清楚就问一次。别的客户端连上不代表这个会话连上。检查实际工具与已有配置；若已有 agbrio_read_app，先实际调用，成功就复用这条已验证连接，不重复添加或登录。不能根据旧对话、网址或已创建授权宣称安装/OAuth 成功。
2. 终端/VS Code：实际检查 claude --version、claude mcp list、claude mcp get agbrio。同地址可复用；缺少才在 PowerShell 添加：${add}。占位符不能执行。Added 只代表配置已保存，不代表已认证；不能静默覆盖不同地址或删除其他服务器。没有 claude 命令时，说明实际限制，把这一条命令交给用户在运行 Claude Code 的电脑操作。
3. 在可交互的 Claude Code 输入 /mcp，选 agbrio，按需登录。本人完成浏览器 OAuth 与同意；没有打开浏览器时，在本机打开客户端提供的登录链接，不索取 token、验证码、回调地址或凭据文件。有效旧登录可以不再弹窗，但仍要实际调用工具证明。非交互模式不能完成登录，须明确请本人完成 /mcp 这一步并等待回复。
4. 桌面 Code：输入框旁 + → Add connectors（或 Connectors → Browse connectors），或 Settings → Connectors，添加当前自定义 MCP 地址并授权；在目标 Code 对话验证工具，不把终端或其他账号成功当成桌面验收。不走 ChatGPT 插件入口。官方说明：https://code.claude.com/docs/en/mcp 和 https://code.claude.com/docs/en/desktop#connect-external-tools。
5. 在当前会话真正发现 agbrio_read_app 并只读调用，报告实际 scope、approvalMode、expiresAt/revokedAt 和一项读取结果，才能说连接完成。HTTP 有响应、配置存在或助手自称已连上都不够。缺少工具时检查 /mcp 状态，通过客户端支持的操作刷新工具，不打断运行中的任务。按缺少 CLI/服务器、传输、OAuth、工具发现、业务参数分层排查，继续修复直到读取成功或明确需要本人的哪一步。
MCP 服务器不一定出现在插件列表，应查看 /mcp 或连接器。Agbrio 的 Claude 桥接插件是另一项功能，用来让 Agbrio 监听/回复 Claude 对话和递送支持的通知；用 MCP 管理 Codex 不要求先装桥接插件。若也要在 Agbrio 监听 Claude，另走 Agbrio 的 Claude 连接流程，不能仅贴提示词就宣称两边都已安装。
只读验收不发测试消息、不建订阅、不启动任务。实际读取通过后再问本人要管理什么，保留现有审阅和回执规则。`;
}
