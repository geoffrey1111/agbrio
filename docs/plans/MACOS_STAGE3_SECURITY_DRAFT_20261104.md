# macOS阶段3安全设计草案 — 待所有者审阅

状态：PARTIALLY_APPROVED / D1_D2_EXCLUDED / D3_REJECTED / D4_D5_D6_APPROVED。日期：2026-11-04，Asia/Singapore。

Owner已审阅本稿：Mac不提供Pi；D1/D2不进入本阶段，D3不批准；D4复用既有Windows发布密钥，D5不做ACL迁移、授权提示由本人点“始终允许”，D6批准。实施顺序为Mac去Pi → D5 → D4 → D6。每批先报告差异/验证，再问是否commit/push到原草稿PR4；不合并、不发布。此设计批准不等于以后每批的Git或Release许可。

## 审阅结论与当前批次

| 项 | Owner结论 | 当前状态 |
| --- | --- | --- |
| Mac去Pi | Mac无Pi入口、无pi-adapter资源、界面不显示Pi；进程组源码可留但不启用 | 第一批本地候选实施/验证中，未提交推送 |
| D1 Pi提权 | 不进入本阶段 | 不实现 |
| D2 Pi全部磁盘工作区 | 不进入本阶段 | 不实现 |
| D3 AX读取 | 不批准，双方现行均退役 | Mac不新增；M20双方均退役 |
| D4 更新签名 | 批准，复用现有Windows发布签名密钥，不新建Mac key | 排在D5后，当前批次不实现、不读私钥、不发布 |
| D5 Keychain | 批准，无ACL迁移，ad-hoc升级后本人点“始终允许”为已知差异 | 排在去Pi后，当前批次不实现 |
| D6 证书/连接信任 | 批准 | 排在D4后，当前批次不实现 |

阶段4改为有Mac的群友协助，不租云Mac。群友仅用自己账号/机器；材料在去Pi→D5→D4→D6完成后准备，不在当前批次抢先制作。结果标为“外部测试者真机验证”，不是所有者UAT；分发只提出pre-release（另批发布批准）与所有者手动转发DMG两种选项，现在无Release。

## 共同边界与模块方向

维持`Core/application → platform adapter → native helper`依赖方向。Core只处理能力/状态/准确原生ID，不接触AX节点、Keychain句柄、launchd载荷或shell脚本。拟新增平台叶子在`src-tauri/src/platform/`和受维护的Mac helper内；Pi、更新器分别拥有各自状态机。此处路径只是设计，尚未创建相应实现。

只允许桌面本人改变提权、目录、TCC、秘密和信任设置；HTTP/PWA/MCP不能配置这些权限。已审阅MCP任务只能在本人预置权限上限内选择/收窄，管理关联不等于调用方身份认证。无泛化root shell、任意executable/argv/env、任意PID停止、全局权限修改或远程force-update入口。

所有变更绑定：本人动作及摘要hash、操作UUID、目标安装/策略修订、native进程UID/实际权限/创建时间/映像、目的和过期时间。先持久化intent再dispatch；已确认失败才回滚，不确定状态只对账原对象，不自动重放。不能用标题、窗口顺序、截图或声称的parent ID证明授权。

现有Codex/Claude启动、model、Goal、原生权限、凭据与活会话不变。新增能力只进新cohort；不迁移或提升当前任务。fixture、静态签名、CI、真实权限、原生账号和USER_UAT分别记录。

## D1：Pi提权 — 本阶段不进入

Mac暂不提供Pi（所有者决定）。不实现密码授权、root helper、SMAppService/root daemon或Pi cohort，不把“不提供”记成已对齐。Windows Pi及现有授权保持；已写Mac继承进程组源码保留不启用。

## D2：Pi全部磁盘工作区 — 本阶段不进入

随着Mac去Pi，不实现Pi全本地卷选择/工作区授权，不请求这项完全磁盘访问。普通附件、看板等现有文件选择及其系统权限独立保留，不因移除Pi删除用户数据或扩大权限。

## D3：AX浏览器读取 — 不批准

Windows正式能力已退役，Mac不新增AX读取、不新增辅助功能请求、不恢复旧扩展或Native Messaging；M20维持双方均退役。前稿相关实现方案以Owner本结论撤回，历史文本保留在ad7cbd8，不作为后续实现入口。

## D4：Mac更新签名与安装事务

Owner已批准复用现有Windows发布签名密钥，不新建Mac专用密钥。任何私钥/口令都不进GitHub secrets、代码、日志或artifact，CI只产Agbrio候选包。Apple Developer账号不是Tauri包签名的替代或前置；没有该账号仍保持ad-hoc，不声称Developer ID/公证。

拟议流程：CI在精确SHA构建`.app.tar.gz`及manifest → 本机核repo/ref/run/artifact/digest/包内架构/版本/签名与文件列表 → 用本地Tauri signer签精确字节 → 本机再验签/篡改负例 → 本人另行批准发布后才更新`latest.json`的`darwin-aarch64`。保留Windows字段与发行流程。Tauri的更新包、`.sig`及OS-ARCH格式是官方已有格式；本轮没有添加plugin、生成key、签名或上传。[Tauri updater](https://tauri.app/plugin/updater/)。

下载地址、平台、SemVer、已批准版本/hash和包内Info.plist都需吻合，不能只信latest.json。提取拒绝穿越/绝对路径/特殊文件/逃逸链接；Mac framework合法相对链接须在签名manifest范围内。暂存/备份与目标在同卷，原子替换、自有目录/安装身份检查、SQLite一致备份与integrity、审计、失败回滚、未知结果围栏均保留。无权限替换`/Applications/Agbrio.app`时另走仅此安装目的的本人授权，不借Pi root relay做通用安装。

按2026-11-02最新Owner规则：没有绑定精确候选的桌面二次确认时，活动或不确定任务继续阻止安装；本人在应用内确认该精确已验证版本后，允许强制安装，不能再以活动/不确定任务阻止。解释Pi中断与手机/MCP暂断，备份后重核候选/身份/确认有效性并保持Pi新准入围栏；这不是每版本再索取聊天确认，也不是远程force-update或自动伪造确认。既有共享Codex独立进程不作为可终止对象。

测试：签名/平台/版本/回滚/不完整下载/路径攻击、备份失败、安装中断、恢复与数据保留；活动/不确定任务的未确认阻止与已确认强装两分支；helper或Keychain更换身份不能静默扩大ACL。实际升级放在隔离Mac，先做工程验收再请本人UAT。

## D5：Keychain与ad-hoc身份

建议当前用户login Keychain的generic password，按email、Events签名key及将来明确必要的秘密分开；非秘密配置只存opaque item reference与版本。仅Agbrio当前用户Host读写，不给root Pi或MCP直接读secret值。后台读取禁止弹窗；Locked/Denied/interaction required保留NeedsOwnerAction，不切明文、环境变量或root keychain。

无账号情况下先评估file-based login Keychain的`SecAccess`/当前调用程序限制；不依赖尚未获得的App Group/Team entitlement，不同时设置互斥的`kSecAttrAccess`和`kSecAttrAccessControl`。Apple说明trustedlist=nil仅信任调用程序；这不是“所有程序可读”。[SecAccessCreate](https://developer.apple.com/documentation/security/secaccesscreate(_:_:_:))、[kSecAttrAccess](https://developer.apple.com/documentation/security/ksecattraccess)。

ad-hoc更新可改变代码身份；这是已知平台差异，不承诺无提示跨版本读取。Owner决定**不做ACL迁移**。系统提示时由受测机器本人点击“始终允许”；Agbrio/agent不代点、不按bundle名称放宽ACL，不为避免提示增加可信应用。拒绝、锁定或未完成提示时保持NeedsOwnerAction，不转明文或自动重发。相关系统行为仍需群友自有Mac实证。[Apple代码签名说明](https://developer.apple.com/library/archive/technotes/tn2206/)。

删除/撤销只作用于当前Agbrio拥有的item，其他账户/钥匙串不碰。邮件发送UNKNOWN不因解锁而自动重发；Events重试与资源通知两分钟实际采样新鲜度照旧，不能借重连续期旧样本。

## D6：证书信任与共享Codex

普通app-server优先保持自有stdio；只有已确认需要独立客户端时再评估私有Unix socket、内核peer UID/原生身份、能力token及连接epoch。不能把官方文档里支持的transport推导成Desktop实包一定采用它；文档亦标记TCP WebSocket为experimental/unsupported，所以不把未认证TCP listener变成正式默认。[OpenAI app-server](https://learn.chatgpt.com/docs/app-server)。

阶段2正式入口继续Unsupported，实包验证移阶段4，由有Mac的外部测试者使用自己的机器和账号、预装或自行正常安装官方客户端，文件只留测试者Mac本地；不再由Agbrio仓库/CI下载，不上传OpenAI包、提取源码/SDK或profile到仓库/artifact。记录最少版本/hash与不含秘密的结果。Owner明确不租云Mac，不申请或产生云Mac费用。

不复制Windows CA、私钥、账户或profile，不静默写login/system trust store。若真实Desktop要求当前stdio/Unix方案无法满足的TLS或私有接口，保持不可用并报告具体证据；不得发明未文档化attach参数、假装Desktop已连接或注册通用CA。只有双方受控客户端已证明支持时才可另提app-scoped pinning；系统信任变更必须单独列用途、证书约束、撤销与本人确认。

## 分批实施与审阅顺序

1. 先将ad7cbd8当前CI等到终态并报告；已整体PASS（Mac两次source/新12 fixture/DMG、Windows build+独立干净安装）。
2. 当前第一批只做Mac不带Pi，先报告最小改法，完成后报告差异/验证并单独问Git提交推送。Windows主配置/资源集合保持，Mac overlay按RFC7396删除pi-adapter资源键；后端Host能力控制桌面/PWA，MCP与直接动作独立拒绝，数据不删。
3. 之后按D5→D4→D6实施，逐批报告、逐批问Git。D4仅本地复用既有签名key，CI只产Agbrio包，发布前另问；当前无Keychain、更新或证书实现。
4. 去Pi/D5/D4/D6完成后，再准备面向群友的中文测试清单：首次打开、逐项操作/预期/失败截图日志、标明需官方Codex/手机的项，包含共享Codex实包；测试者提交前自查个人路径、账号、对话。这里只记录后续要求，尚未制作清单或联系群友。
5. 分发给Owner两个选项：另获批准的公开pre-release，或Owner手动转发DMG。现在不发布任何Release。DEVLOG/对齐表记录“外部测试者真机验证”，不写所有者UAT。

设备试用身份M11另保留待办；不随本批收集硬件身份。根权限和磁盘访问的Pi设计已退出本阶段，不能从旧草案恢复。

## 来源核对与未证实项

以上链接于本轮核对；Apple Developer的JS壳页改读其官方Markdown。对无账号SMAppService身份、root/TCC继承与ad-hoc跨版本Keychain不确定处均明确留原生硬门，没有依据搜索摘要宣称已可用。本轮无任何阶段3运行验证。
