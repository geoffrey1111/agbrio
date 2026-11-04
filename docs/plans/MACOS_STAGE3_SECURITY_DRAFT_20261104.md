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

## 2026-11-04 D5第一批实施候选（尚未提交）

前序Mac去Pi已验证：6a7c986两条Mac source与Windows build/独立clean install全部SUCCESS。当前D5拆为先平台叶子+邮件，再Events密钥生命周期；后者完成前不称D5完成、不进入D4。

第一批使用已锁定的core-foundation 0.10.1、security-framework 3.7.0和sys 2.17.0，在Host内调用Security.framework。Core新增native-secret-references-v1迁移，只存邮件账号哈希和v1随机UUID条目引用。先预留引用后写Keychain，意外退出可按同一引用重试；没有明文/环境变量替代。当前用户UID必须普通且与effective UID相同，login.keychain-db通过当前系统用户目录定位，固定service和随机条目引用，并显式限定Keychain查询范围，不搜索System/default全局列表。

新条目使用SecAccessCreate(trustedlist=nil)，既有条目只更新值，不修改ACL/可信应用或访问组；不要求Developer Team entitlement。Apple TN3137说明data-protection需要授权entitlement，file-based与SecItem shim有差异；本项目沿用Owner批准的file-based方案，不能把官方示例或Windows测试当Mac运行证明。来源：[TN3137](https://developer.apple.com/documentation/technotes/tn3137-on-mac-keychains)、[SecAccessCreate](https://developer.apple.com/documentation/security/secaccesscreate(_:_:_:))、[SecItemUpdate](https://developer.apple.com/documentation/security/secitemupdate(_:_:))。

后台read和delete检查锁状态并使用AuthenticationUIFail；Locked/Denied保留NEEDS_OWNER_ACTION。桌面新增AUTHORIZE_EMAIL_KEYCHAIN，显式读取可展示原生授权，返回值只有AUTHORIZED、不回传secret、不发邮件/不开提醒/不补发旧记录；PWA既有desktop-setup拒绝边界保留。失败删除保留准确引用，不假称已清除。ad-hoc升级提示本人点“始终允许”，不做ACL迁移，仍需阶段4外部测试者真机验证。

受维护的原生读写/锁定/精确删除测试仅限GitHub临时Mac runner，创建专用临时keychain，固定假密码，不读取login keychain、不主动设置默认钥匙串或全局search list；结束删除准确自有keychain并断言成功。该测试与新Mac代码尚未获本批push许可、尚未运行。Windows契约/迁移/界面结果单列，不能提升为Mac Keychain已通过。

Owner追加：D6后与群友清单一起准备公开仓库中文Mac部署说明和用户可交给自己Codex的指令：架构检查、精确版本/SHA源码、构建安装；系统安全和Keychain提示由本人操作。当前不抢先制作、不合并、不发布Release。

## 2026-11-04 D5第二批Events候选（渲染已验，未提交，新Mac CI待验）

第一批8b4076431843f8ac7de0f7854f43729990c1cb2a工程CI已全通过：Mac两条source、专用临时Keychain原生fixture1项；Windows build和独立clean install通过。该证据不等于真实账号/ad-hoc升级/群友真机验证，也不替代本第二批的Mac检查。

第二批把Mac Events签名密钥存入与邮件分离的固定generic-password service，数据库只存版本化随机引用、grant/subscription绑定及生命周期元数据。Windows DPAPI原函数和既有事件发送重试预算与8b完全相同。新表039_mcp_native_secret_refs在原生Add前预留；Add完成后由同一个blocking操作结算，即使HTTP等待者取消也不丢回执。相同目标的未完成创建保持围栏，并沿用每grant32个订阅的限制；同密钥续期重用引用，轮换保留既有五分钟重叠。

保存订阅在同一SQLite事务中核对引用/主体/状态/创建结算；SQL trigger与现有轮换、撤销、到期和级联删除原子登记退役。既有Events worker在发送步骤结束后处理准确引用的清理，不新增服务、不影响当前投递lease，不重置通知预算。删除receipt绑定lease与revision，晚到创建使旧receipt失效；创建意图不是条目归属证明，只有原生Add已确认成功的条目才允许原生删除。冲突/未创建只完成元数据回收，未确认创建保留待核对状态和最少ID围栏，不删除可能外来的条目。已删除引用保留无秘密的最少tombstone，防止重用/误复活。

后台原生创建/读/删都不允许系统弹窗。桌面新增只返回metadata的status和明确authorize IPC；只有本人点击才能解锁本用户login Keychain、读取已确认拥有的条目并请求系统授权，不输出secret、不创建订阅、不发送测试通知。输入cursor在原生操作前校验，授权按64项分页，手机号/PWA/MCP不提供该授权入口。code不修改已有ACL或trusted applications，ad-hoc“始终允许”仍由机器本人操作。

当前本地Windows候选完整Core330/7ignored、前端875/109、Host417/69ignored通过；私有Core345/7ignored通过。包含请求方取消后晚到创建、旧删除receipt与新结算、拒绝/未确认创建不进行原生删除等故障测试。原空间20失效后Owner已明确批准恢复；在新空间24用公开导出候选完成1440桌面授权分页/拒绝、390手机隐藏与Windows隐藏检查，无横向溢出。只访问本地模拟页，无真实Keychain/账号/通知操作；这是AUTOMATED_UI_EMULATION，不是真机或USER_UAT。新Mac编译和扩展双service原生fixture未执行，须新批报告/Git授权后CI。D5尚未整体完成，D4/D6及阶段4交付物未开始。

## 2026-11-04 D4第一批：无私钥产包/本地签验签工具候选（未提交）

D5的b98924f两条Mac source和Windows build/独立clean install已经全部SUCCESS；D5工程门闭合，真实账号/ad-hoc升级仍阶段4。下述是独立、未提交的D4a工具准备，不是D4完成。应用内Mac更新check/download/install的Unsupported门未解除，Windows运行时更新和Windows workflow原样。

Mac package workflow候选新增unsigned updater artifact：在已有严格ad-hoc/ARM64、DMG交付/隔离启动验证之后，将精确Agbrio.app打为版本化.app.tar.gz与MACOS_UPDATE_MANIFEST.json，归一uid/gid/mtime，不带签名私钥。manifest绑定source SHA、版本、darwin-aarch64、压缩包SHA和关闭的类型/mode/hash文件清单。解析器验证app标识/Info.plist版本与13.5下限、主程序/Node ARM64 Mach-O和真实load-command最低系统；拒绝绝对/穿越/大小写Unicode别名、特殊文件、硬链接、悬空/逃逸/父路径symlink、privileged modes、Pi资源。允许Framework受限相对symlink链。打包/ZIP准备不覆盖原产物，失败独占create不删除另一writer刚创建的输出。

sign-macos-update.ps1只在本机Windows PowerShell7、非GITHUB_ACTIONS运行：按明确repo/ref/SHA/run/artifact取当前成功Mac package provenance，核ZIP digest，安全解包/核tar+manifest后才读既有Windows agbrio-updater.key与DPAPI password。没有generate、上传、Release或安装命令。私钥路径/密码只给本地固定Tauri CLI签名子进程；不透传给Cargo验证，stdout/stderr不作为敏感日志输出，调用者环境保持原样。新增ignored Rust gate复用固定公钥验证原始归档并拒绝改字节，必须出现精确hash的PASS marker（避免0测试exit0误通过）。真实签名尚未执行，只有fixture验证和隔离测试，不能记已签或可发布。

Mac fragment生成器只产本地candidate。若合并Windows manifest，必须同版本和固定repo/asset，保留Windows及其他平台字段与notes/date；不覆盖live latest.json。不提供新的Mac release key。第一批没有修改public key或凭据，也没有读私钥/DPAPI文件；真实归档、签名与篡改实证须本批获准push后先由MacCI产物再本地执行。发布另问。

本地候选：归档/ZIP/来源/危险路径/文件归属/最低系统11项PASS；Mac fragment/保留Windows2项PASS；PS隔离/CI拒绝/PASS marker checks PASS；现有更新器9PASS/4ignored（含新增实际Mac签名gate未执行）。Mac workflow语法可解析不是实际运行。新Mac archive与签名流程须批准新批Git后CI/本机核验。安装事务会另批实现：当前锁定updater2.12.0默认Mac install没有满足批准的同卷/回滚边界，不能直接解除门；不借Pi relay、不静默信任CA、不宣称Developer ID/公证。
