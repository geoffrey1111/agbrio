# Agbrio

**电脑上的 Agent 继续工作，你在手机上掌握进展。**

[![Download](https://img.shields.io/github/v/release/geoffrey1111/agbrio?label=download)](https://github.com/geoffrey1111/agbrio/releases/latest)
[![Build](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml/badge.svg?branch=main)](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml)
[![MIT](https://img.shields.io/github/license/geoffrey1111/agbrio)](LICENSE)
[![Windows](https://img.shields.io/badge/Windows-x64-0078D4)](https://github.com/geoffrey1111/agbrio/releases/latest)

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

查看 Codex、Claude Code 对话，发消息跟进，接收新回复和待处理通知。把管理与执行分工，也可以让一个管理对话协调多个执行对话。

![桌面对话总览与执行状态](docs/assets/zh-CN/conversations-current.png)

## 离开电脑，也能看进度、回消息

一眼看到执行中、等待确认和子对话状态。点进原对话查看成果、补充要求、回答支持的提问，不用远程控制整个桌面。

<p align="center"><img src="docs/assets/zh-CN/mobile-conversations-current.png" width="300" alt="手机对话列表与子对话状态"> &nbsp; <img src="docs/assets/zh-CN/mobile-reply-current.png" width="300" alt="在手机回复原对话"></p>

## 选择对话，开始监听或绑定协作

从电脑上已有的对话中选择。监听一个对话，随时跟进；把规划与执行连成 Bridge，选择要转交的内容，确认目标后由你批准发送。

<p align="center"><img src="docs/assets/zh-CN/select-conversation-current.png" width="300" alt="选择电脑上的已有对话进行监听"> &nbsp; <img src="docs/assets/zh-CN/review-current.png" width="300" alt="发送前审阅内容与目标"></p>

![选择 Bridge 的管理端与执行端](docs/assets/zh-CN/binding-current.png)

## 把日常跟进交给管理对话

授权一个管理对话协调多个 Codex 执行对话：读取进展、传递任务、汇总结果；遇到需要你决定的事项，再交给你。管理对话可以来自 Claude Code，也可以来自 Codex。

```mermaid
flowchart TD
  U["你 · 方向与关键决策"] <--> M["管理对话 · Claude Code / Codex"]
  M <--> A["Codex · 界面实现"]
  M <--> B["Codex · 接口开发"]
  M <--> C["Codex · 测试验收"]
```

管理委派支持向 Codex 发送；Claude Code 对话支持你本人从手机回复。关键决策和授权范围始终由你掌握。

## 开始使用

1. 下载 Windows 版，连接你正在使用的 Agent。
2. 选择对话开始监听，或绑定管理端与执行端。
3. 在「设置 → 设备」配对手机，打开你的 Agbrio 地址。

手机访问时，电脑与 Agbrio 需要保持在线；通知需在设备上启用。

[Download](https://github.com/geoffrey1111/agbrio/releases/latest) · [使用帮助](docs/START_HERE.md)

<details>
<summary>还有这些顺手的功能</summary>

直接阅读对话中的文档、图片与项目看板。需要自定义集成时，再使用 API / MCP。

</details>

<sub>所有截图均为当前界面，使用虚构示例数据。</sub>


## macOS 预览候选（Apple Silicon）

macOS 适配仍在进行，尚未与 Windows 功能对齐。手动 **macOS candidate DMG** Actions 生成 ad-hoc 签名 `.dmg` artifact，不建 Release、不使用更新签名 secret。ARM64 源码测试及较早候选 DMG 的交付副本启动已在 GitHub macOS 26.6.2 通过。macOS 13.5 是声明的部署目标，尚未在该系统版本验证。

原生路径、本机维护 CLI、Claude 引擎发现、通知集成、菜单栏/Dock、登录启动配置、资源采样、限定用户服务恢复、固定版本按需 Chromium 已有 Mac 实现及自动化测试。真实账号、系统权限弹窗、通知点击、手机端到端与长期运行仍待验证。共享 Codex 已有按 app-server 协议编写的准备/激活/关闭内部候选及模拟测试；正式 Mac 入口在阶段4群友自有 Mac 实包验证前继续不支持。仓库/CI 不再下载或分发 OpenAI 安装包。Mac 按所有者决定不提供 Pi（无入口及适配器资源）。Keychain 秘密、设备试用身份、辅助功能和签名应用内更新需要后续安全设计批准。已退役的浏览器连接器/Native Messaging 保持退役。

核对 artifact 校验和后，将 `Agbrio.app` 放入 Applications。本人信任的 ad-hoc 候选若被拦截，可到**系统设置 → 隐私与安全性 → 仍要打开**，或由本人执行下方命令。应用不会自动移除 quarantine。无需 Apple Developer 账号；候选没有 Developer ID 签名或公证。

```text
xattr -dr com.apple.quarantine /Applications/Agbrio.app
```

通知权限只由明确的桌面操作申请。所选文件、可移动卷或受保护位置可能需要本人授予文件与文件夹或完全磁盘访问权限。未来只读 AX 功能才需辅助功能权限。ad-hoc 身份随每次构建变化，更新后系统可能要求重新授权。CI 通过是自动化工程证据，不代表用户 UAT 或完整功能对齐。
