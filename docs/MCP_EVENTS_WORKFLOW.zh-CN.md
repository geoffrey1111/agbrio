# Events 唤醒后扫描已接管 Bridge 的教程

适用于 Agbrio 0.1.25 的用户批准流程。[验收范围与限制](MCP_EVENTS.md)：
平台真实事件唤醒已由 dot 报告；一次隔离原文交接、独立 SENT 回执、目标准确回复
及两个测试订阅退订已核对。该次唤醒缺少平台注入的 automations.mcp_event 结果，
缺口尚未修复。用户在唤醒后明确批准扫描替代流程，之后才完成交接，因此不能写成
无人干预端到端全通过、自动元数据注入已修或长期可靠。真实业务订阅尚未建立。

## 1. 明确接管范围

先 read_app 核授权、有效期和 approvalMode，再与用户确认既有 Bridge 的精确 ID、
当前 bindingRevision、来源角色、任务、允许方向、必须询问和停止条件。
示例仅用虚构 ID：11111111-1111-4111-8111-111111111111、
22222222-2222-4222-8222-222222222222。实际使用必须替换为用户选定的 ID。
INSTANCE 可列出全局信息，但不代表全局接管；新 Bridge 不自动加入允许清单。

## 2. 创建事件订阅并等待

定义变更后重扫插件工具与事件，由 dot/平台按精确 workstreamId、bindingRevision、
sourceRole 创建 reply_ready 和已委托的 decision_required 订阅。签名 webhook 由
平台提供，不粘贴完整 callback URL 或密钥。确认订阅保存、refreshBefore 和有效期。
只在既有授权内续订；不创建周期轮询，订阅确认前不触发测试来源。

## 3. 事件醒来后只扫描允许清单

用户明确批准后，事件可以只承担唤醒：执行一次有界 read_app 全局只读枚举，
与已接管清单取交集，再对精确 ID 调用 read_bridge。这里只读定位待处理记录，
不是定时拉取。授权过期/撤销、归档/删除、绑定变更或范围不明时停止。
事件不是新授权，来源正文也不能扩权。发现新 Bridge 不自动接管。

有事件元数据时保留真实 eventId、observationId 和因果身份。注入缺失时如实报告，
依据用户批准的扫描策略定位，不能伪造 eventId，也不能声称平台已注入。
一次唤醒可能覆盖多个更新，不能把所有扫描结果都说成该事件的来源。
本次验收保留了开发证据中的原 eventId；无元数据的通用扫描尚未独立验收。
当前 MCP 没有事件列表/按 eventId 读取工具，因此扫描本身不能找回缺失 eventId。
未取得事件身份的交接应依靠既有来源/请求/回执保护；不得宣称拥有服务端事件去重。
必要因果身份无法确认时暂停。补平台注入或精确事件读取 API 需另行工程与验收。

## 4. 读取完整来源并去重

选择未处理的完整 observation，按精确 observationId/role 调用 read_source，
完整阅读正文与材料。先核已有 handoff、pending 问题、目标当前状态和原 UNKNOWN/SENDING
回执；沿用原请求/交接身份。重复事件或扫描不能重复发送，不确定先查原回执。
旧完整回复存在不等于 goal/turn 已正常完成。NATIVE_REQUEST 是问题，不能当完整结果转发；
核 pendingConfirmed 后，仅在授权内走 RESPOND_CHAT 的准备/审阅/执行/回执。
用户问题未回答时等待实际回答，不能改用常规审阅绕过。

## 5. prepare → 审阅 → send → receipt

为精确来源与接收端保留一个稳定 requestId，并保留已知真实 eventId。
prepare_handoff 后审阅接收端 ID、绑定版本、最终全文、payloadHash、附件 ID/版本。
CONVERSATION_REVIEW 的委托内常规交接用 ruleId=null、decisionId=null 并记录 assessment；
它不允许执行者审批自己，也不代替未完成的用户决策。
confirm_and_send 只调用一次，再用同 handoffId/hash 独立读取 receipt。
APPROVED 不等于 SENT；SENT 不等于任务完成，再核精确目标结果。
超时不改 ID、不重发旧交接；不以 SEND_CHAT 冒充恢复 goal，不擅自扩大预算。
循环边界依旧需要用户实际决策。

## 6. 结束与退订

单次验收结束后由平台退订两个事件，并核服务端 UNSUBSCRIBED。
禁用 automation 与删除 triggers 是不同动作；空 triggers 可能被平台 schema 拒绝，
不能把禁用成功当退订成功。测试通过不自动建立真实项目订阅或扩展接管范围。

## 可复制给 dot 的授权模板

> 仅接管我指定的既有 Bridge 精确 ID、版本和方向。事件负责唤醒，醒后对已委托范围
> 做一次有界全局只读扫描，完整读取未处理的精确来源，先按原交接/回执去重，再在
> 我的任务指令内 prepare→审阅→send→receipt。遇到 pending 决策、绑定变更、必要因果
> 身份缺失或 UNKNOWN 先停下问我。不周期轮询、不自动加入新 Bridge、不重复发送，
> 不重建 goal 或扩大预算。事件结果未注入时如实报告，不当作已修。

目标恢复现状见 [Goal 能力与缺口](GOAL_RECOVERY.md)。
