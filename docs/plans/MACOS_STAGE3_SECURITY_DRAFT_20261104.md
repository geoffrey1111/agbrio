# macOS阶段3安全设计草案 — 待所有者审阅

状态：DRAFT / NOT_APPROVED / NO_STAGE3_IMPLEMENTATION。日期：2026-11-04，Asia/Singapore。

本轮只设计下列六项，没有写阶段3产品代码、安装helper、请求提权/TCC、生成或读取签名私钥、写Keychain或修改证书信任。阶段2共享Codex三步候选是本轮另行授权的普通协议实现，不能被当作阶段3已经批准。

## 审批摘要

| 决策 | 建议批准的范围 | 本次不承诺的事 |
| --- | --- | --- |
| D1 Pi提权 | 无开发者账号的桌面本人密码授权、短生命周期专用cohort；先验证身份链，再开放 | 不声称ad-hoc已能安全部署SMAppService常驻root服务 |
| D2 全部磁盘 | 当前本地卷UUID快照、目录范围修订、原生访问与TCC双重检查 | 根目录选择或root身份不等于完全磁盘访问已获准 |
| D3 AX浏览器读取 | 本人显式启用的前台来源只读适配器，逐浏览器支持清单 | 不恢复旧连接器、不做导航/输入/自动权限点击 |
| D4 Mac更新签名 | 独立的本地加密Mac发布签名密钥，CI只产Agbrio更新包，人工本地签名 | 不把Tauri签名当Developer ID/公证，不自动发布 |
| D5 Keychain | 当前用户login Keychain，按用途分项、后台禁止弹窗、拒绝不降级 | 不明文落盘，不授权所有程序，不读其他程序账户 |
| D6 证书信任 | 共享后端优先stdio；确需本地通信再审Unix socket，默认不装CA | 不复制Windows证书/私钥，不假设Desktop支持自定义pin或根信任 |

这是供本人选择/修改的建议，不是已批准决定。批准设计后仍需分批实现、自测，再逐批申请提交推送。任何需要更宽信任、不可证明身份或收费的路线必须再次形成具体审阅项，不能借本表自动扩权。

## 共同边界与模块方向

维持`Core/application → platform adapter → native helper`依赖方向。Core只处理能力/状态/准确原生ID，不接触AX节点、Keychain句柄、launchd载荷或shell脚本。拟新增平台叶子在`src-tauri/src/platform/`和受维护的Mac helper内；Pi、更新器分别拥有各自状态机。此处路径只是设计，尚未创建相应实现。

只允许桌面本人改变提权、目录、TCC、秘密和信任设置；HTTP/PWA/MCP不能配置这些权限。已审阅MCP任务只能在本人预置权限上限内选择/收窄，管理关联不等于调用方身份认证。无泛化root shell、任意executable/argv/env、任意PID停止、全局权限修改或远程force-update入口。

所有变更绑定：本人动作及摘要hash、操作UUID、目标安装/策略修订、native进程UID/实际权限/创建时间/映像、目的和过期时间。先持久化intent再dispatch；已确认失败才回滚，不确定状态只对账原对象，不自动重放。不能用标题、窗口顺序、截图或声称的parent ID证明授权。

现有Codex/Claude启动、model、Goal、原生权限、凭据与活会话不变。新增能力只进新cohort；不迁移或提升当前任务。fixture、静态签名、CI、真实权限、原生账号和USER_UAT分别记录。

## D1：Pi提权与身份

### 无账号主路线（拟议，尚须原生可行性验证）

建议先采用本人在本机终端发起的固定bootstrap + macOS原生密码授权，准备一次受验证的root执行cohort，再由桌面复用。终端只允许固定helper入口和受约束的请求UUID，不接受远程命令、环境或脚本正文。密码只进入系统授权界面/终端原生认证，不经过Agbrio、模型、日志或MCP。取消/拒绝保持未准备；不静默启动普通权限替代品。

bootstrap读取本人私有目录下的不可变请求摘要；root侧重新核对目录/文件所有权、模式、链接、时效、nonce，以及原始Agbrio进程的audit identity、UID、PID版本/创建时间、实际代码身份和已批准安装hash。不能只信SUDO_USER、路径字符串、bundle ID或调用者自报UID。无开发者账号时尤其不能把可重签的ad-hoc名称当跨版本身份锚。

目标记录分开保存owner UID、real/effective UID及root身份。可评估由root bootstrap设置owner real UID、effective UID0的新Pi进程，但这些UID和exec后的实际结果必须用系统接口验证；环境变量不是证明。只传固定Node/Pi入口，继承本人已有native配置/资源引用，不复制auth文件到root HOME、不更改provider/模型/原生审批。Pi的终端能力仍受本人原策略和审阅合同约束。

root relay只做prepare/attest/正常关闭这一个cohort；Pi业务执行仍走既有Pi协议。更新安装必须用独立目的授权，不能复用Pi root权利安装任意包。现有继承进程组不构成OS沙箱；新session/daemon化逃逸是明确的完成门，未解决前不能宣称可靠回收全部后代或开放Mac Pi。

### 路线比较与硬门

Apple在macOS13+提供SMAppService管理bundle内helper，注册LaunchDaemon需管理员批准，且与后续启动/开机生命周期有关。[SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice)、[register](https://developer.apple.com/documentation/servicemanagement/smappservice/register())。

它可以作为后续稳定签名路线的候选，但当前没有证明ad-hoc下的安装身份/跨版本升级/客户端鉴别满足要求，不能直接采用或自动注册开机root服务。已弃用的API不作为默认路线；密码对话框也不替代调用方鉴别。若无账号bootstrap无法闭合原生身份链，停止该路线并保持Unsupported，交回具体风险与替代设计，不取消校验。

实现门：本人批准D1 → 一次性无业务授权实验 → 真UID/有效root/代码身份/同owner验证 → 取消/拒绝/超时/PID重用/跨用户/链接/过期/重入 → 精确cohort关闭与逃逸后代策略 → 新Pi功能准入。不得用mock密码成功替代这组实证。

## D2：全部本地磁盘、目录授权与TCC

“全部磁盘”指本人保存时可枚举的本地卷集合；记录卷UUID、当前挂载点、只读状态、用途和policy revision，不用盘符、卷名或路径前缀单独授权。根卷与`/Volumes`本地卷可入集合，网络卷拒绝；嵌套挂载/重新挂载/UUID变化逐次检查，不能因位于获准目录下就包含网络文件系统。新卷默认不扩大旧cohort；本人保存新集合后仅新cohort采用。

普通目录可用NSOpenPanel选取与适用的bookmark；实际访问仍检查POSIX权限、TCC及当前文件/卷身份。scope记录不是OS授权，bookmark也不保证整个机器可访问。[Apple bookmark选项](https://developer.apple.com/documentation/foundation/nsurl/bookmarkcreationoptions/withsecurityscope)。

全卷上限与既有受保护根策略先做集合相减。保护Agbrio控制/授权/收据、native会话与凭据、系统/应用树，不能用根卷前缀吞掉这些禁区；若现有Windows规则与全卷语义冲突，明确列出而非直接移植盘符判断。文件操作采用原生句柄与volume/file identity重核，拒绝symlink逃逸、挂载点替换与硬链接绕过；read/write分开，任务审阅hash绑定scope revision。

完全磁盘访问由本人在系统设置决定；不编辑TCC数据库、模拟开关或用root身份伪装已获授权。拒绝/撤销时报告具体访问能力不可用，保留原任务/草稿和UNKNOWN，不自动重启/重发。root helper与Pi实际responsible process是否继承相应授权必须在原生Mac逐项验证，不能从Host可读推出child可读。[Apple文件访问说明](https://support.apple.com/en-ca/guide/security/secddd1d86a6/web)；本次该Apple Support正文限流，未据此宣称具体继承规则已验证。

测试：根/外接本地卷/网络与嵌套挂载、同名异UUID、移除重挂、只读卷、链接/替换、受保护根、授权授予/拒绝/撤销、root与标准child实际访问、旧cohort不被扩权，全部使用隔离fixture，不写本人游戏工程。

## D3：AX浏览器只读适配器

现有普通浏览器与Native Messaging仍退役。本设计若获批准，只新增独立AX来源能力，不恢复扩展安装或用AX驱动页面。建议首批逐一验证Safari与Chromium浏览器，其他浏览器明确Unsupported，不能以一个浏览器成功泛化。

仅本人点击启用时允许请求辅助功能；平常状态读取/后台轮询不弹提示。AXIsProcessTrustedWithOptions的提示是异步动作，返回false不能被当成已授权。[Apple AX授权接口](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions)。

读取目标限定为本人当前前台浏览器：核实际UID/PID/出生时间/映像/bundle及当前AX窗口。只读取获准的URL必要属性；无AX写属性、performAction、按键、点击、导航、切换tab、聚焦、DOM/CDP、cookies、历史或密码字段。限制树深/节点数/字符串大小和请求时间；不遍历所有应用窗口。身份变化、权限撤销或结构不匹配立即停止读取，不尝试“修复”本人浏览器。

URL投影先剔除凭据和敏感query/fragment，账号/OAuth回调不外发；提供方适配器只按可信canonical URL里的原生conversation ID关联。标题可不是身份，也不使用OCR/截图路由。原始AX树不落盘，不经MCP泛化成任意桌面读取。

完成门：本人批准恢复这项只读来源 → 授予/拒绝/撤销 → 多窗口/隐私窗口/错PID/换进程/非支持浏览器 → URL红线/只读API清单与真实浏览器验证；自动化证据不写USER_UAT。

## D4：Mac更新签名与安装事务

建议Mac使用独立、本地加密的发布签名密钥；也可选择复用现有Windows发布key，但需要本人明确接受跨平台信任范围。任何私钥/口令都不进GitHub secrets、代码、日志或artifact，CI只产Agbrio候选包。Apple Developer账号不是Tauri包签名的替代或前置；没有该账号仍保持ad-hoc，不声称Developer ID/公证。

拟议流程：CI在精确SHA构建`.app.tar.gz`及manifest → 本机核repo/ref/run/artifact/digest/包内架构/版本/签名与文件列表 → 用本地Tauri signer签精确字节 → 本机再验签/篡改负例 → 本人另行批准发布后才更新`latest.json`的`darwin-aarch64`。保留Windows字段与发行流程。Tauri的更新包、`.sig`及OS-ARCH格式是官方已有格式；本轮没有添加plugin、生成key、签名或上传。[Tauri updater](https://tauri.app/plugin/updater/)。

下载地址、平台、SemVer、已批准版本/hash和包内Info.plist都需吻合，不能只信latest.json。提取拒绝穿越/绝对路径/特殊文件/逃逸链接；Mac framework合法相对链接须在签名manifest范围内。暂存/备份与目标在同卷，原子替换、自有目录/安装身份检查、SQLite一致备份与integrity、审计、失败回滚、未知结果围栏均保留。无权限替换`/Applications/Agbrio.app`时另走仅此安装目的的本人授权，不借Pi root relay做通用安装。

按2026-11-02最新Owner规则：没有绑定精确候选的桌面二次确认时，活动或不确定任务继续阻止安装；本人在应用内确认该精确已验证版本后，允许强制安装，不能再以活动/不确定任务阻止。解释Pi中断与手机/MCP暂断，备份后重核候选/身份/确认有效性并保持Pi新准入围栏；这不是每版本再索取聊天确认，也不是远程force-update或自动伪造确认。既有共享Codex独立进程不作为可终止对象。

测试：签名/平台/版本/回滚/不完整下载/路径攻击、备份失败、安装中断、恢复与数据保留；活动/不确定任务的未确认阻止与已确认强装两分支；helper或Keychain更换身份不能静默扩大ACL。实际升级放在隔离Mac，先做工程验收再请本人UAT。

## D5：Keychain与ad-hoc身份

建议当前用户login Keychain的generic password，按email、Events签名key及将来明确必要的秘密分开；非秘密配置只存opaque item reference与版本。仅Agbrio当前用户Host读写，不给root Pi或MCP直接读secret值。后台读取禁止弹窗；Locked/Denied/interaction required保留NeedsOwnerAction，不切明文、环境变量或root keychain。

无账号情况下先评估file-based login Keychain的`SecAccess`/当前调用程序限制；不依赖尚未获得的App Group/Team entitlement，不同时设置互斥的`kSecAttrAccess`和`kSecAttrAccessControl`。Apple说明trustedlist=nil仅信任调用程序；这不是“所有程序可读”。[SecAccessCreate](https://developer.apple.com/documentation/security/secaccesscreate(_:_:_:))、[kSecAttrAccess](https://developer.apple.com/documentation/security/ksecattraccess)。

ad-hoc更新可改变代码身份；不能承诺旧item自动被新包读取。首选本人通过系统Keychain对精确新版本重新授权，或在本机重新输入秘密；如将来做ACL迁移，必须额外绑定已验证新包的代码要求、仅此item和本人动作，不能Allow all或按bundle名称放宽。调用者身份与升级前后授权均为实现硬门。[Apple代码签名说明](https://developer.apple.com/library/archive/technotes/tn2206/)。

删除/撤销只作用于当前Agbrio拥有的item，其他账户/钥匙串不碰。邮件发送UNKNOWN不因解锁而自动重发；Events重试与资源通知两分钟实际采样新鲜度照旧，不能借重连续期旧样本。

## D6：证书信任与共享Codex

普通app-server优先保持自有stdio；只有已确认需要独立客户端时再评估私有Unix socket、内核peer UID/原生身份、能力token及连接epoch。不能把官方文档里支持的transport推导成Desktop实包一定采用它；文档亦标记TCP WebSocket为experimental/unsupported，所以不把未认证TCP listener变成正式默认。[OpenAI app-server](https://learn.chatgpt.com/docs/app-server)。

阶段2正式入口继续Unsupported，实包验证移阶段4。阶段4由云Mac上的本人正常取得/安装官方客户端，文件只留该Mac本地；不再由Agbrio仓库/CI下载，不上传OpenAI包、提取源码/SDK或profile到仓库/artifact。记录最少版本/hash与不含秘密的结果。租赁费用仍需本人另行批准，本轮没有租赁。

不复制Windows CA、私钥、账户或profile，不静默写login/system trust store。若真实Desktop要求当前stdio/Unix方案无法满足的TLS或私有接口，保持不可用并报告具体证据；不得发明未文档化attach参数、假装Desktop已连接或注册通用CA。只有双方受控客户端已证明支持时才可另提app-scoped pinning；系统信任变更必须单独列用途、证书约束、撤销与本人确认。

## 分批实施与审阅顺序

1. 本人审阅D1–D6及状态表；批准前无阶段3实现。
2. 若批准，先实现纯验证/状态与隔离负例，再做D1/D2原生授权硬门。无法证明无账号身份链就停该子项，不让其他功能绕过它。
3. Keychain先当前用户边界，再研究ad-hoc升级授权；AX只在明确批准恢复只读来源后实现；更新器先离线验签/包/备份/围栏，再隔离真实安装。
4. 每批列精确路径/差异/验证后，另问是否允许提交推送到现有codex/macos-support/PR4；保持草稿，不合并/发布。
5. 阶段4清单包含共享Codex真实包三步/准确原生ID/不重复写入/独立会话保护、所有TCC拒绝撤销、root与目录交集、通知、真实升级/Keychain身份变化。mock不能替代这些；费用与账号动作由本人决定。

设备试用身份M11等不在本轮六项实施批准内，仍保留待办，不能随此草案默认收集硬件身份。

## 来源核对与未证实项

以上链接于本轮核对；Apple Developer的JS壳页改读其官方Markdown。对无账号SMAppService身份、root/TCC继承与ad-hoc跨版本Keychain不确定处均明确留原生硬门，没有依据搜索摘要宣称已可用。本轮无任何阶段3运行验证。
