<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">

# Agbrio

**Agent Bridge · 让管理对话与执行对话顺畅交接**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

如果你已经习惯让一个 Agent 对话管理项目，另一个对话实际执行，Agbrio 就是围绕这种用法设计的。

管理对话保留规划、审查结果、决定下一项任务；执行对话在项目里实现修改，返回结果与证据。**Bridge 负责把选定的指令、结果和材料在这两条已有对话之间交接，每次发送都由你确认。**

手机让你可以随时审阅和确认这次交接。产品的重点是两条对话之间的桥梁，远程访问是到达这个工作台的方式。

## 发布状态

**正在准备开源发布。** 个人 Windows／Codex 实现已在使用，作者已反馈手机转发成功。目前这个公开仓库提供项目介绍、配图与自部署方案；**尚未发布应用源码、安装包、可用的安装命令或选定的开源许可证**。

## 适合已经使用双对话协作的人

这是为主动分开管理与执行上下文的人准备的工作流工具。两条对话和两端角色由你选择。Agbrio 不自动分配 Agent，也不让管理 Agent 自行批准自己的结果。

**指令 → 执行端；结果与证据 → 管理端；最终确认 → 由你完成。**

## 一个对话管理，一个对话执行，Bridge 连接交接过程

![Managing and executing conversations side by side in one Bridge](docs/assets/desktop-bridge.png)

<p><img src="docs/assets/mobile-handoff.png" width="320" alt="Management-to-execution instruction selection"> <img src="docs/assets/mobile-return.png" width="320" alt="Execution-to-management result selection"></p>

电脑图展示同一个 Bridge 的双端对照；两张手机图分别展示「管理端向执行端下发指令」和「执行端向管理端交回结果」的选择界面。使用真实组件、虚构数据和浏览器渲染，不包含私人对话，也不代表设备验收。应用控件目前为中文，多语言 README 不代表应用已完成多语言适配。

## 个人版本目前能做什么

- **Bridge 双端协作** — 绑定已有的控制端与执行端对话。查看任意一端，按相反方向把执行结果交回控制端。
- **选择内容后交接** — 阅读完整原文，整块勾选需要的段落，修改发送内容，看清接收端，最后一次确认发送。指令块识别只是建议，发送什么由你决定。
- **带齐材料** — 选中的 Codex 交接文件会复制到接收端项目的 `.aiwr/incoming/<handoff-id>/` 目录，消息附上相对路径和 SHA-256。接收端读取的是实际本地副本；这不等于把文件上传成 ChatGPT 对话附件。
- **通知与原对话回复** — 打开具体的结果通知，直接回复对应的原对话。公开的用户消息和 Agent 回复从旧到新排列，更早的消息在上方加载。
- **桌面 Host ＋手机 PWA** — 电脑端保存绑定、观察结果和发送记录，手机使用配对后的网页会话。实时读取和发送需要电脑开机、保持唤醒并联网。

## 一次完整交接

1. 创建 Bridge，绑定准确的控制端和执行端对话。
2. 阅读控制端回复，选择指令块与需要的材料。
3. 核对接收端，点击一次「确认发送」。
4. 执行端返回结果后，审阅并选择内容，再交回控制端。

## 手机如何连接电脑

![Agbrio conceptual network and handoff diagram](docs/assets/connection-flow.svg)

概念结构：手机操作电脑 Host，Host 连接两端 Agent 对话。「HTTPS 入口」代表用户选择的网络服务，不是 Agbrio 托管的账号服务。

目前个人环境使用 Cloudflare Tunnel 提供 HTTPS 入口。Cloudflare 负责网络路径，应用状态和请求仍由电脑 Host 处理。PWA 可以先显示缓存界面，但缓存内容不能证明电脑当前在线。

后续自部署流程计划允许用户选择自己的连接方式。下面列的是规划选项，当前应用还没有可用的通用选择器。

- **Cloudflare Tunnel** — 适合已有 Cloudflare 托管域名的用户。
- **Tailscale Funnel** — 计划提供使用服务商域名的公网 HTTPS 入口，无需购买自己的域名。
- **Tailscale Serve** — 计划提供私有网络入口，两端都需要使用 Tailscale。
- **自己的 HTTPS 代理或隧道** — 计划供已有远程访问配置的用户使用。

[自部署设计与 Agent 配置指令草稿](docs/SELF_HOSTING_PLAN.zh-CN.md)

## 确认、数据与送达

- 使用准确的 conversation／thread 标识绑定目标，不按标题、截图或界面位置猜测。
- 每次交接需要用户明确确认，不建立自动循环的管理端／执行端工作流。
- 送达不确定时保留该状态，先核对记录，不盲目重复发送。
- 选定文件的真实字节与哈希才是材料依据；只显示源电脑的路径，不能让远程接收端自动拿到文件。
- 自部署仍需信任选用的隧道／网络服务商和 Agent 服务商；手机缓存和本地留存的消息也是私人数据。

## 当前边界

当前个人使用路径是 Windows ＋ Codex。ChatGPT 集成仍属实验性功能，需要另行验证账号会话和完整收发链路。Claude Code、其他 Agent、macOS 与通用自部署流程均是后续工作，不是已发布的支持承诺。iPhone PWA 的布局和渲染问题仍在排查。目前不提供托管的多用户服务。

## 可用开源版本发布前

- 导出干净的源码，审查依赖和分发条件后确定许可证。
- 实现连接方式选择、连接验证和可交给 Agent 的部署指令。
- 在全新 Windows 环境验证安装、配对、恢复和文件交接；Mac 独立验证。
- 通过独立适配器扩展其他 Agent，分别验证真实读取与发送行为。

## 联系作者与反馈

[作者主页](https://github.com/geoffrey1111) · [反馈问题](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml) · [提出工作流建议](https://github.com/geoffrey1111/agbrio/issues/new?template=feature_request.yml)

使用问题也可以在 Issues 中交流。这里是公开渠道，请使用示例数据，并在发布前移除私人内容。
