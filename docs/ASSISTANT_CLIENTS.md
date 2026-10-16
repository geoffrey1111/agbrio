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
