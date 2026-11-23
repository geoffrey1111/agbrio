# Agbrio Windows 0.1.35-rc.160

Changes since the public rc.154 release:

- Question answers use their exact delivery receipt to update the card and header immediately. A stale read no longer replaces a confirmed SENT display with another request to answer. State refresh continues in the background; uncertain writes are never resent automatically.
- The mobile review-board proxy accepts the bounded `queue=1` parameter with a supported review scope. UI resource candidates and images can load through the authenticated phone route. Unknown parameters, duplicate query keys and unapproved write routes remain blocked.
- Active INSTANCE assistant connections can create and manage Pi conversations inside existing owner-authorized workspace ceilings. Expiry, revocation, action review and directory boundaries remain enforced.
- Reviewed INSTANCE text messages can reach existing Claude Code sessions through the authenticated plugin. Independent native receipts, input leases, explicit FIFO queues and UNKNOWN fences remain in place.
- Includes conversation chronology and pasted-image handling, management activity/freshness improvements, opt-in management reminders, Codex observation recovery, model/effort validation and current-user CLI update support developed after rc.154.

## Validation and limits

The signed rc.160 installer is the same package prepared for local installation. Its updater signature, tamper rejection, native executable/version and all 93 bundled frontend resources were verified. The final question-card change passed 53 focused frontend tests and TypeScript/build checks. Earlier focused Pi, native input/queue and model validation checks are retained in the local release evidence. This is not a new full-suite, macOS or physical-iPhone acceptance claim.

The review route was checked in the installed rc.159 Host through an authenticated browser: the previously blocked request returned HTTP 200 with 10 candidates and rendered images. The rc.160 question-card change has not yet received physical-device acceptance.

This release fixes misleading delivery feedback and the extra foreground refresh wait. It does **not** establish the cause of all desktop/mobile latency or intermittent upstream MCP HTTP failures. A measured slow answer took about eight seconds to reach native confirmation; no automatic retry or weakened uncertainty protection was added.

Local upgrades stop the old Agbrio Host and may interrupt its tasks. Stored history is preserved, but model execution does not automatically resume. Use the owner-run updater at a suitable time. The macOS beta and existing Mac branches are unchanged.

## 简体中文

- 修复答复已送达后，问题卡和顶部仍显示“等待确认”的误导反馈；后续完整刷新在后台进行。
- 修复手机看板候选队列请求被拦截，恢复 UI 候选和图片读取。
- Pi 工作区面向所有有效 INSTANCE 助手连接，继续保留审核、撤销、目录边界和 UNKNOWN 保护。
- 包含 Claude 双向文本通信、独立回执、显式排队、消息排序、粘贴图片及管理区状态等 rc.154 之后的改进。
- 不宣称全部卡顿或上游 MCP 瞬态故障已经根治。更新会短暂退出旧 Agbrio，请选择合适时间；不会自动重投不确定消息。

The release includes the Windows installer, detached updater signature, update manifest, exact tagged source archive and SHA-256 checksums. No local profile, pairing data, conversations or signing credentials are distributed.
