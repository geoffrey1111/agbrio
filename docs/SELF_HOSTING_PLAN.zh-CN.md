# Agbrio — Agent Bridge：开源版设置与部署

> 历史规划。2026-10-06 源码候选版已经实现电脑端方式选择、验证保存与复制部署
> 指令；请以 [SETUP.md](SETUP.md) 的现行操作为准。Tailscale 账号开通仍未作真实
> 账号验收，首个安装包发布仍等待干净 Windows 验证。

用户已确定品牌 Agbrio，副标题 Agent Bridge。当前是正式规划；连接方式选择
与通用部署尚未实现，不把现有个人 Cloudflare 实例当作多服务商验收。

## 设置流程

电脑「设置 → 手机连接」：选择方式 → 配置 → 验证 → 手机配对。
已配置用户直接看到状态、地址/二维码和设备列表，更换方式放次级操作。

- Cloudflare Tunnel：使用已有域名/隧道，保留当前个人配置。
- Tailscale Funnel：提供固定HTTPS地址，手机只需PWA。
- Tailscale Serve：私有入口，电脑/手机均加入自己的Tailscale网络。
- 自有HTTPS入口：验证已有反向代理或其他隧道。

主区域只有方式、必要字段和一个主操作；「交给 Agent 配置」复制当前方式
对应的完整指令，预览页呈现长文本，不把说明堆满设置页。
状态：未配置 / 配置中 / 待登录 / 验证中 / 已连接 / 连接中断。
验证成功后显示二维码和配对操作；已登录设备仍按现有90天规则续期。
域名变更影响Cookie、草稿和推送注册，界面应准确说明重新配对范围。
准备新配置时保持旧连接，验证后再提交；失败恢复旧配置。

## 实施前提

DesktopWebAccess 当前没有选择方式/写配置API；web_availability恢复逻辑写死
Cloudflared；MobileHttpConfig还要求Cloudflare Access字段。因此单加下拉框
不能称为支持Funnel。明确建模方式、地址、配对鉴权、可选Access和恢复策略。
其他服务商不强制Cloudflare Access；精确Host/Origin及TLS校验仍保留。
切换到Tailscale后不得仍修复/启动Cloudflared。外部登录使用用户自己的账号。
Quick Tunnel只作临时体验：重建变地址，不默认用作长期PWA入口。

## 界面复制的 Agent 指令模板

```text
请在我的电脑上配置 Agbrio（Agent Bridge）的手机远程访问。
连接方式：{{connection_method}}
安装/源码目录：{{install_directory}}
已核实Host端口：{{verified_local_port}}
已有HTTPS地址（可选）：{{public_https_origin}}

先读当前版本README、AGENTS和部署文档，核对操作系统、安装方式及真实Host
配置。未知端口/路径/API先查证，不编造默认值或尚未实现的接口。
直接完成可自动化的依赖安装、配置、后台运行和检查。
保留旧连接、登录设备与对话数据；新配置先验证后提交，失败恢复旧配置。
Cloudflare用我的现有域名/隧道；Funnel用我的Tailscale账号固定HTTPS地址；
Serve只在我的私有网络开放；自有入口核对我给定的HTTPS地址。
需要服务商登录、授权或管理员操作时说明原因和下一步，让我完成。
不要让我把密码、Cookie、私钥或token粘贴进对话，优先用官方登录流程。
只配置Agbrio的入口，保留其他服务和Agent会话。重启正在工作的Host/Agent
前说明影响，等待我选择时机；不启动第二个Codex Desktop。
保留Host/Origin、配对鉴权及TLS校验，不通过关掉鉴权证明连通。
分别验证本机Host、公网HTTPS和未登录写入拒绝。用临时测试设备，结束注销；
回复、附件、推送跳转用临时测试对话，不向日常对话发测试消息。
最后返回手机地址/二维码入口、配对方法、实际通过的检查、待我完成的步骤
和回滚方法。浏览器模拟不等于iPhone真机验收。
```

模板只带非敏感配置，不包含配对码、凭据、数据库或浏览器状态。
待发布版文档/API确定后再标为可完整执行，避免让Agent猜尚不存在的接口。

## 验收与新仓库

重复配置幂等、失败回滚、真实远程回复/附件/推送深链接/重连、未配对写入
拒绝、服务商/系统权限失败可恢复；Windows/Mac分别记录真实验证范围。
独立仓库agbrio，不继承旧Git历史。先发布新写的介绍，再按白名单导出源码。
发布前清理个人域名/路径/账号和真实对话等资料；不携带.aiwr、runtime、data、
profile、凭据、构建缓存或Codex Desktop二进制。核验许可证和第三方分发条件。
新用户文档只保留现行流程；未经验证的Agent/平台标为计划。首次源码发行前
完成无个人配置的新机安装和一条真实远程交接。当前个人实例保持兼容。
