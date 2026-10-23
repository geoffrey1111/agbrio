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
