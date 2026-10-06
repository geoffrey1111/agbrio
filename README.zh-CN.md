<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">

# Agbrio

**Agent Bridge · 让管理对话与执行对话顺畅交接**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

如果你已经习惯让一个 Agent 对话管理项目，另一个对话实际执行，Agbrio 就是围绕这种用法设计的。

管理对话保留规划、审查结果、决定下一项任务；执行对话在项目里实现修改，返回结果与证据。**Bridge 负责把选定的指令、结果和材料在这两条已有对话之间交接，每次发送都由你确认。**

手机让你可以随时审阅和确认这次交接。产品的重点是两条对话之间的桥梁，远程访问是到达这个工作台的方式。

## 发布状态

**[v0.1.0-alpha.1](https://github.com/geoffrey1111/agbrio/releases/tag/v0.1.0-alpha.1) — MIT 开源 Windows Alpha。** 可下载安装包与校验值，或从源码构建。干净 Windows 安装和原生首次设置已通过自动验证，真实远程 Codex／文件交接也已单独验证。应用支持简体中文、繁体中文和英文，可在「设置 → 语言」切换。Mac 与其他 Agent 尚未作为支持平台。[安装与配对](docs/SETUP.md)。

## 适合已经使用双对话协作的人

这是为主动分开管理与执行上下文的人准备的工作流工具。两条对话和两端角色由你选择。Agbrio 不自动分配 Agent，也不让管理 Agent 自行批准自己的结果。

**指令 → 执行端；结果与证据 → 管理端；最终确认 → 由你完成。**

## 一个对话管理，一个对话执行，Bridge 连接交接过程

![Managing and executing conversations side by side in one Bridge](docs/assets/desktop-bridge.png)

<p><img src="docs/assets/mobile-handoff.png" width="320" alt="Management-to-execution instruction selection"> <img src="docs/assets/mobile-return.png" width="320" alt="Execution-to-management result selection"></p>

电脑图展示同一个 Bridge 的双端对照；两张手机图分别展示「管理端向执行端下发指令」和「执行端向管理端交回结果」的选择界面。使用真实组件、虚构数据和浏览器渲染，不包含私人对话，也不代表设备验收。示例截图使用中文界面；应用可切换简体中文、繁体中文和英文。日文、韩文目前仅为介绍文档语言。

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

![Agent Bridge topology](docs/assets/connection-flow.svg)

[安装与配对](docs/SETUP.md) · [源码构建](docs/BUILD.md) · [MIT 许可证](LICENSE)

电脑「设置 → 设备」可选 Cloudflare Tunnel、Tailscale Funnel、私有 Tailscale Serve 或已有 HTTPS 入口。填入地址后先验证 TLS 和本机 Host 身份，再保存；可复制部署指令交给自己的 Agent。Funnel/Serve 无需购买域名，使用自己的服务商账号。Tailscale 真实账号开通链路仍需单独验证，不提供 Agbrio 托管中转。

## 确认、数据与送达

- 使用准确的 conversation／thread 标识绑定目标，不按标题、截图或界面位置猜测。
- 每次交接需要用户明确确认，不建立自动循环的管理端／执行端工作流。
- 送达不确定时保留该状态，先核对记录，不盲目重复发送。
- 选定文件的真实字节与哈希才是材料依据；只显示源电脑的路径，不能让远程接收端自动拿到文件。
- 自部署仍需信任选用的隧道／网络服务商和 Agent 服务商；手机缓存和本地留存的消息也是私人数据。

## 当前边界

首批范围：Windows x64＋Codex＋手机 PWA。界面支持简体中文、繁体中文和英文。Mac、其他 Agent 和 ChatGPT 浏览器链路不作为已支持发行功能。用户对 iPhone 修复的反馈与新机器安装验证分别记录。

## 构建与参与

从[安装文档](docs/SETUP.md)或[构建文档](docs/BUILD.md)开始。保留精确对话身份、最后一次确认和未知送达状态的核对。项目使用 [MIT](LICENSE)，依赖保留[各自的许可及声明](THIRD_PARTY_NOTICES.md)。

## 联系作者与反馈

[作者主页](mailto:geoffreyzjx@qq.com) · [GitHub](https://github.com/geoffrey1111) · [反馈问题](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml) · [提出工作流建议](https://github.com/geoffrey1111/agbrio/issues/new?template=feature_request.yml)

使用问题也可以在 Issues 中交流。这里是公开渠道，请使用示例数据，并在发布前移除私人内容。
