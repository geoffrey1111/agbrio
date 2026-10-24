## Answering Codex questions through MCP

Use `agbrio_read_chat` and copy the card's exact `requestId`, integer `revision`, and every `questions[].id`. Both `state.requests` and `state.asyncQuestions` use the reviewed `RESPOND_CHAT` operation:

```json
{
  "requestId": "unique-action-request-id",
  "operation": "RESPOND_CHAT",
  "input": {
    "threadId": "exact-native-thread-id",
    "requestId": "exact-card-request-id",
    "input": {
      "revision": 1,
      "answers": {"exact-question-id-from-card": "Selected option or written answer"}
    }
  }
}
```

The outer `requestId` identifies the prepared action; the nested `requestId` identifies the question card. The innermost `input` is required. Async question IDs are JSON-encoded strings: preserve them exactly, rather than replacing them with the option label, number or array. `revision: 1` above is an example: always copy the current card revision. Native permission cards instead use `input: {"revision": <current revision>, "decision": "<offered decision>"}`. Submit exactly one of `answers` or `decision`.

Prepare, review, execute, and inspect the action receipt using the existing grant. Async answers share the owner's durable answer ID and native structured answer wrapper; active controllable turns use guidance, idle turns send, unavailable guidance never queues implicitly. Already-answered, changed or expired cards are rejected. Do not substitute a plain SEND_CHAT for a failed response, invent an owner decision, or resend an answer already delivered. Argument errors identify the bad field without echoing answer content; `receiptRequired: false` applies to these pre-prepare format errors only. Actual uncertain writes still require their receipt.

## 2026-10-24 — Create an executor and follow up immediately

An authorized INSTANCE assistant can prepare CREATE_CHAT with provider:CODEX, the exact existing project directory and an optional display label. Review and execute once, then retain the returned threadId/watch.generation. Creation sends no first task; use a separate reviewed SEND_CHAT for the actual delegated work. Empty conversations are not promised to survive a backend restart before the first real task. Inspect the original action receipt after uncertainty, never create again to replace an unknown result.

For SEND_CHAT, use AUTO by default: guide the active exact Codex turn or send while idle. Queue only when the owner explicitly asked to wait, with mode:QUEUE and queueRequested:true. FAILED with delivery:NOT_SENT is a proven blocked/pre-write result; UNKNOWN is an unresolved native attempt. Both keep their original terminal request identity. Refresh MCP tool discovery after updating an older Host so the client sees CREATE_CHAT/current policy. This adds no Claude MCP write/creation or unattended orchestration.

> 新用户先读：[AI 助手与 MCP 图解入门](tutorials/AI_ASSISTANT_QUICKSTART.zh-CN.md)。应用内「设置 → AI 助手」提供同样的原理图、使用示例和分步连接教程。


> 新用户先读：[AI 助手与 MCP 图解入门](tutorials/AI_ASSISTANT_QUICKSTART.zh-CN.md)。应用内「设置 → AI 助手」提供同样的原理图、使用示例和分步连接教程。

# 通用 AI 助手连接与 WorkBuddy

2026-10-17 开发版本；工程验收与实际用户接通分别记录。现有 Dot 连接、OAuth 令牌、授权、Events 订阅和任务不因新增客户端而重建。

## 连接方式

设置 → AI 助手保留 ChatGPT/dot 和本地 Codex，新增 WorkBuddy、其他 MCP 助手。选择实际客户端后复制对应指令。支持 Streamable HTTP + OAuth/DCR/PKCE 的客户端可连接；只有聊天或打开网页的能力不足以安装 MCP。

WorkBuddy 官方说明：<https://open.workbuddy.cn/docs/connector>。在自定义连接器中添加实例的 HTTPS `/mcp` 地址，使用 OAuth。配置文件方式只合并到 `~/.workbuddy/mcp.json` 的 `mcpServers`，不能覆盖其他服务或复制其他助手的凭据：

```json
{
  "mcpServers": {
    "agbrio": {
      "type": "streamableHttp",
      "url": "https://your-instance.example/mcp"
    }
  }
}
```

此为配置示例，没有 token、客户端密钥或真实用户地址。由本人完成登录与同意；优先复用适当且有效的原授权，但不共享客户端令牌，不将旧单 Bridge 授权升级。其他客户端字段格式以其官方说明为准。

## OAuth 兼容

- HTTPS 和本机 HTTP 回环回调继续支持。
- WorkBuddy 原生回调仅允许 `workbuddy://workbuddy/mcp/connector%3A<source>/oauth/callback`；source 限小写字母、数字、连字符，首字符必须字母或数字，长度 1–128。禁止用户信息、端口、额外查询、fragment、编码路径逃逸、路径穿越或其他协议。DCR 保存原始字符串，授权及换码精确匹配。官方也说明客户端可能回退一次到 HTTP 回环；旧版不能仅凭首次私有协议失败判定不可用。
- 新注册且明确请求 `refresh_token` 的客户端可获得轮换刷新令牌；不请求或旧持久化注册默认保持授权码模式。旧访问令牌及订阅不重写、不缩期。
- 刷新令牌只存 SHA256；与客户端、资源、原授权和令牌族绑定；原租期不因刷新延长。访问令牌最多一小时，且不晚于原授权到期。撤销、过期、改绑仍由原授权检查拒绝。
- 正确身份下重放已使用的刷新令牌会撤销该族及其关联访问令牌；不撤销共享授权、其他客户端的独立令牌或任何订阅。错误客户端/资源/扩大 scope 不消耗合法刷新令牌。丢失刷新响应后不能盲目重试：重新走本人同意，不触碰原业务发送。
- 原生 OAuth 回调与事件 webhook 是两个边界。此变更不放宽 webhook 的 HTTPS、公网地址、DNS、challenge 或签名校验。

## 三种能力分别验收

| 能力 | 必需证据 |
| --- | --- |
| 按需操作 | 目标助手真实发现工具；`agbrio_read_app` 返回正确 INSTANCE、approvalMode、有效期和撤销状态；读取本人指定 Bridge 与完整来源。工具数以实际发现为准。 |
| 事件持续托管 | 另获授权，客户端具备事件订阅/签名 webhook 和真实唤醒运行时；无轮询、无人发送消息情况下实际唤醒并完成一次隔离授权交接。仅 HTTP 2xx、手机推送或 tools/list 不算通过。 |
| 定时日报 | 本人明确提交配置，真实定时工具创建/更新并回读同一任务；运行时可用连接工具。没有能力则明确未启用。 |

新客户端默认只做连接与只读验收，不新增订阅、轮询、日报或业务发送。不得抢占 Dot 已有 Bridge 写入职责；需要重新分工由本人明确决定。没有事件运行时的助手仍可按需操作，不能声称后台持续监听。

交接继续 `read_bridge/read_source → prepare_handoff → 审阅精确接收端/最终文字/hash/附件版本 → confirm_and_send → 独立 receipt`。稳定 requestId、observationId、bindingRevision、去重、UNKNOWN 和待答问题 Gate 不变。SENT 不代表下游完成。

## 用户自行继续开发

可将 [给本机开发助手的完整指令](tutorials/WORKBUDDY_DEVELOPMENT_PROMPT.zh-CN.txt) 原文发给拥有本机仓库工具的助手。它要求先证据诊断、保留 WIP/连接，再实现和验收，不会仅凭提示词声明已接通。不要附凭据或完整 OAuth URL。

## 当前证据边界

对方用户的 WorkBuddy 版本、Agbrio 版本、原始错误及授权结果尚未提供；其真实接通和事件运行时未验收。开发中的自动化 HTTP/数据库/UI 测试只能证明各自覆盖的工程路径，不替代实际 WorkBuddy OAuth/工具调用，更不代表任意客户端已支持持续托管。
