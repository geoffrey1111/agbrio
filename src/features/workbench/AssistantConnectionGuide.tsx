import {ClaudeNativeConnection} from "./ClaudeNativeConnection";
import type {ClaudeConnectionResult} from "./ClaudeSessionPicker";
import {useId,useRef,useState} from "react";
import {ArrowLeft,ArrowRight,Check,ExternalLink,Link2,ShieldCheck} from "lucide-react";
import {t,getLanguage,useLanguage} from "../../i18n";
import {CopyAction} from "./NativeReaderActions";
import {assistantSetupInstructions,assistantReadOnlyInstructions,assistantTakeoverInstructions,type AssistantRoute} from "./assistantSetupInstructions";
import "../../styles/assistant-guide.css";
import {AssistantDailyReport,type ReportBridge} from "./AssistantDailyReport";
import {claudeCodeAddCommand} from "./claudeConnectionInstructions";
import {assistantMcpConfig} from "./assistantClientInstructions";
import {AssistantOverview} from "./AssistantOverview";

type ExistingGrant={id:string;scope?:string;approvalMode?:string;label:string;expiresAt:number;revokedAt:number|null};
export type AssistantHelpPage="CHATGPT_PLUGINS"|"AUTH_DOCUMENTATION";
function ClaudeCodeConnectionSteps({mcpUrl,connect}:{mcpUrl:string|null;connect?:()=>Promise<ClaudeConnectionResult>}){
 const command=claudeCodeAddCommand(mcpUrl);
 return <section className="assistant-step-context" aria-label={t("Claude Code 完整接入")}>
  <h3>{t("Claude Code 完整接入")}</h3>
  <ol>
   <li><strong>{t("添加 MCP 服务器")}</strong><p>{t("终端或 VS Code：先运行 claude mcp get agbrio 检查；没有这条连接时，再复制下面的 PowerShell 命令。")}</p>{command?<div className="assistant-guide-prompt"><CopyAction showLabel text={command} label={t("复制添加命令")}/><pre>{command}</pre></div>:<p>{t("请先配置不含凭据或查询参数的 HTTPS /mcp 入口。")}</p>}<p>{t("桌面 Code：输入框旁 + → Add connectors，或 Settings → Connectors，添加自定义 MCP 地址。须在实际使用的对话验证。")}</p></li>
   <li><strong>{t("本人登录与同意")}</strong><p>{t("终端输入 /mcp，选择 agbrio 完成登录；桌面端在连接器中授权。已有有效登录可以复用，不一定再次弹窗。")}</p></li>
   <li><strong>{t("实际调用才算连接成功")}</strong><p>{t("让助手实际调用 agbrio_read_app，报告权限范围、有效期和读取结果。复制指令、保存配置或已有授权记录，都不等于这个对话已连上。")}</p></li>
  </ol>
  <p>{t("MCP 连接请看 /mcp 或连接器，不一定出现在插件列表。要让完成通知回到 Claude，或从手机回复 Claude 原对话，继续完成下方的本机连接即可。")}</p>
  <ClaudeNativeConnection connect={connect}/>
  <a href="https://code.claude.com/docs/en/mcp" target="_blank" rel="noopener noreferrer">{t("Claude Code 官方接入说明")}</a>
 </section>;
}
export function AssistantConnectionGuide({mcpUrl,grants,bridges=[],onAddGrant,onRouteChange,openHelp,connectClaude}:{mcpUrl:string|null;grants:ExistingGrant[];bridges?:ReportBridge[];onAddGrant:()=>void;onRouteChange:(route:AssistantRoute)=>void;openHelp:(page:AssistantHelpPage)=>Promise<void>;connectClaude?:()=>Promise<ClaudeConnectionResult>}){
 useLanguage();const id=useId(),heading=useRef<HTMLHeadingElement>(null);
 const[route,setRoute]=useState<AssistantRoute|null>(null),[step,setStep]=useState(0),[openError,setOpenError]=useState("");
 const active=grants.filter(g=>g.scope==="INSTANCE"&&g.approvalMode==="CONVERSATION_REVIEW"&&!g.revokedAt&&g.expiresAt>Date.now());
 let https=false;try{const u=new URL(mcpUrl??"");https=u.protocol==="https:"&&!u.username&&!u.password;}catch{/* Unconfigured address is not a connection. */}
 const universal=route==="claude"||route==="workbuddy"||route==="generic";
 const target=route==="cloud"?"dot":route==="local"?"Codex":route==="claude"?"Claude Code":route==="workbuddy"?"WorkBuddy":t("当前助手");
 const config=assistantMcpConfig(mcpUrl);
 const labels=["检查入口","准备授权",route==="local"?"添加到 Codex":universal?"添加 MCP 连接":"注册云端插件","本人同意","只读验收","选择 Bridge"];
 function choose(value:AssistantRoute){setRoute(value);setStep(0);setOpenError("");onRouteChange(value);}
 function navigate(value:number){setStep(value);setOpenError("");requestAnimationFrame(()=>heading.current?.focus({preventScroll:true}));}
 async function external(page:AssistantHelpPage){setOpenError("");try{await openHelp(page);}catch{setOpenError(t("未能打开浏览器，请复制下方官方网址，在浏览器中打开。"));}}
 return <section className="r2-assistant-tutorial" aria-labelledby={`${id}-title`}>
  <h2 id={`${id}-title`} className="r2-sr-only">{t("连接你的助手")}</h2>
  <div id={`${id}-body`}>
   <AssistantOverview/>
   <p className="assistant-route-question">{t("你要连接哪类助手？")}</p>
   <div className="assistant-route-options" role="group" aria-label={t("助手类型")}>
    <button type="button" aria-pressed={route==="cloud"} onClick={()=>choose("cloud")}><span>ChatGPT / dot {t("云端")}</span></button>
    <button type="button" aria-pressed={route==="local"} onClick={()=>choose("local")}><span>Codex {t("本地")}</span></button>
    <button type="button" aria-pressed={route==="claude"} onClick={()=>choose("claude")}><span>Claude Code</span></button>
    <button type="button" aria-pressed={route==="workbuddy"} onClick={()=>choose("workbuddy")}><span>WorkBuddy</span></button>
    <button type="button" aria-pressed={route==="generic"} onClick={()=>choose("generic")}><span>{t("其他 MCP 助手")}</span></button>
   </div>
   {!route?<p className="assistant-route-hint">{t("选择要使用的助手。每个客户端需要分别连接并验收。")}</p>:<>
    <p className="assistant-guide-notice">{t("连接后可按需操作 Bridge；持续托管和日报需分别核对事件唤醒、定时能力，不会自动开启。")}</p>
    <div className="assistant-agent-start"><CopyAction showLabel text={assistantSetupInstructions(mcpUrl,getLanguage(),route)} label={t("复制给 AI 助手")}/><p>{t("把指令粘贴到所选助手的对话中；它会先检查入口，再引导连接。登录和同意仍由你完成。")}</p></div>
    {route==="claude"&&<ClaudeCodeConnectionSteps mcpUrl={mcpUrl} connect={connectClaude}/>}
    <div className="assistant-current-connection"><p>{t("当前 MCP 地址")}</p>{mcpUrl?<div className="assistant-guide-value"><code>{mcpUrl}</code><CopyAction text={mcpUrl} label={t("复制 MCP 地址")}/></div>:<p>{t("尚未配置 MCP 入口。先到设备设置选择已有 HTTPS 服务或部署方式。")}</p>}{active.length>0&&<p className="assistant-grant-summary"><Check size={18}/>{t("有效授权将直接复用 · 可随时撤销")}</p>}</div>
    <details className="assistant-human-details"><summary>{t("查看我需要做的步骤")}</summary><section className="assistant-human-steps" aria-label={t("只需要你完成")}><h3>{t("只需要你完成")}</h3>
     <ol><li><strong>{t("选择助手授权")}</strong>{active.length?<p>{t("已有有效的整个应用授权，可直接复用。")}</p>:<button type="button" onClick={onAddGrant}>{t("创建 30 天授权")}</button>}<p>{t("旧版单 Bridge 授权不会自动升级。")}</p></li>
     <li><strong>{t("本人登录并同意连接")}</strong>{route==="cloud"&&<button type="button" onClick={()=>void external("CHATGPT_PLUGINS")}>{t("在浏览器打开插件首页")}</button>}<p>{t(route==="cloud"?"按 agent 指引添加云端连接，核对范围并同意，再在目标 dot 中启用。":universal?"在所选助手添加 MCP 连接，完成本人 OAuth 同意，再只读验收工具。":"按 agent 指引连接本地 Codex，核对范围并同意。")}</p></li>
     <li><strong>{t("告诉助手接管哪些 Bridge")}</strong><p>{t("说明任务、需要询问的情况和暂停条件；不需要另填 Brief。")}</p></li></ol>
    </section></details>
    {openError&&<p role="alert">{openError}</p>}
    <details className="assistant-prompt-preview"><summary>{t("预览要复制的指令")}</summary><pre>{assistantSetupInstructions(mcpUrl,getLanguage(),route)}</pre></details>
    <AssistantDailyReport bridges={bridges} mcpUrl={mcpUrl}/>
    <details className="assistant-manual"><summary>{t("自己操作：查看详细教程")}</summary>
    <p className="assistant-overview-note">{t("已有正常连接？先跳到“只读验收”。下面的数字只表示正在阅读的步骤，不表示连接成功。")}</p>
    <nav className="assistant-guide-steps" aria-label={t("连接教程步骤")}>{labels.map((label,index)=><button key={index} type="button" aria-current={index===step?"step":undefined} aria-controls={`${id}-step`} onClick={()=>navigate(index)}><span className="assistant-step-number">{index+1}</span><span>{t(label)}</span></button>)}</nav>
    <div className="assistant-guide-content" id={`${id}-step`}>
     <p className="assistant-guide-count">{t("步骤 {0} / {1}",step+1,labels.length)}</p><h3 tabIndex={-1} ref={heading}>{t(labels[step])}</h3>
     <div className="assistant-step-context"><p><span>{t("操作位置")}</span><strong>{t(step===0?"Agbrio · 设置 → 设备":step===1?"Agbrio · 设置 → AI 助手":step===2?"所选助手的 MCP / 连接器设置":step===3?"浏览器中的 Agbrio 授权页":"所选助手的对话")}</strong></p><p><span>{t("完成后核对")}</span>{t(["拿到当前实例的完整 MCP 地址；云端助手需要可访问的 HTTPS 入口。","看到有效的整个应用授权和到期时间；这一步尚未连接客户端。","助手已添加 agbrio 服务器；若提示登录，继续下一步。","授权成功并返回所选助手；工具能否使用还需下一步检查。","助手实际调用 agbrio_read_app，回报权限、有效期和读取结果。","你已明确交代任务范围、必须询问的情况和暂停条件。"][step])}</p></div>
     {step===0&&<><p>{t(route==="cloud"?"先复用现有部署。云端助手需要能访问这个完整 HTTPS MCP 地址。":universal?"复用现有 HTTPS MCP 入口。按所选助手实际支持的 OAuth 流程连接。":"使用当前实例的实际 MCP 地址。在 Codex 本地完成的认证，只证明这条本地连接。")}</p>{mcpUrl?<div className="assistant-guide-value"><Link2 size={18}/><code>{mcpUrl}</code><CopyAction text={mcpUrl} label={t("复制 MCP 地址")}/></div>:<p className="assistant-guide-notice">{t("尚未配置 MCP 入口。先到设备设置选择已有 HTTPS 服务或部署方式。")}</p>}<p className="assistant-guide-notice">{t(route==="cloud"&&mcpUrl&&!https?"当前地址不是 HTTPS，不能标为云端可达。":"已有可用入口时，不需要重新部署、重启或配对。")}</p><p className="assistant-guide-note">{t("地址来自你的实例配置；已配置地址不等于网络可达。")}</p></>}
     {step===1&&<><p>{t("选择现有有效的整个应用授权；没有时，再由本人添加。")}</p>{active.length?<div className="assistant-guide-existing"><ShieldCheck size={20}/><div><strong>{t("可复用的整个应用授权")}</strong>{active.map(g=><p key={g.id}>{g.label}<span>{t("至")} {new Date(g.expiresAt).toLocaleDateString(getLanguage())}</span></p>)}</div></div>:<button type="button" className="r2-action" onClick={onAddGrant}>{t("创建 30 天授权")}</button>}<p>{t("范围为 INSTANCE，会话审阅模式，可随时撤销。旧版单 Bridge 授权不会自动升级。")}</p><p className="assistant-guide-notice">{t("授权允许访问；还需要在目标助手完成 OAuth 连接。")}</p></>}
     {step===2&&(route==="claude"?<ClaudeCodeConnectionSteps mcpUrl={mcpUrl} connect={connectClaude}/>:universal?<><p>{t(route==="workbuddy"?"在 WorkBuddy 的自定义连接器中添加 Agbrio，选择 Streamable HTTP 和 OAuth。":"在助手的 MCP 设置中添加 Agbrio，使用其支持的 Streamable HTTP 和 OAuth 流程。")}</p>{route==="generic"?<div className="assistant-guide-fields"><p><span>{t("服务器地址")}</span><code>{mcpUrl??t("从步骤 1 复制")}</code></p><p><span>{t("认证方式")}</span><strong>OAuth · DCR · PKCE S256</strong></p><p><span>{t("请求范围")}</span><code>agbrio:instance</code></p></div>:config?<div className="assistant-guide-prompt"><CopyAction showLabel text={config} label={t("复制 MCP 配置")}/><pre>{config}</pre></div>:<p className="assistant-guide-notice">{t("请先配置不含凭据或查询参数的 HTTPS /mcp 入口。")}</p>}<p>{t(route==="workbuddy"?"配置文件方式：只合并到 ~/.workbuddy/mcp.json 的 mcpServers，保留其他连接。连接时由内置 OAuth 管理器处理授权，无需填写客户端密钥。":"配置格式以客户端官方说明为准。未支持 MCP 或 OAuth 时，说明缺少的能力，不能仅凭提示词完成连接。")}</p><p className="assistant-guide-notice">{t("此配置不含 token。不要复制其他助手的凭据、删除 Dot 连接或改动其订阅。")}</p></>:route==="cloud"?<><p>{t("在真正的浏览器打开 ChatGPT 插件首页，选择添加自定义 MCP 服务器。")}</p><button type="button" className="r2-action" onClick={()=>void external("CHATGPT_PLUGINS")}><ExternalLink size={17}/>{t("在浏览器打开插件首页")}</button><div className="assistant-guide-fields"><p><span>{t("服务器地址")}</span><code>{mcpUrl??t("从步骤 1 复制")}</code></p><p><span>{t("认证方式")}</span><strong>OAuth · DCR</strong></p><p><span>{t("请求范围")}</span><code>agbrio:instance</code></p><p><span>{t("客户端密钥")}</span><strong>{t("不填写")}</strong></p></div><p>{t("其他 OAuth 端点自动发现。创建插件后，安装并在目标 dot 中启用。")}</p><p className="assistant-guide-notice">{t("仅限桌面端的插件不会自动成为云端连接；保留它，再注册云端插件。")}</p><p className="assistant-guide-note">https://chatgpt.com/plugins</p></>:<><p>{t("在 Codex 的插件或 MCP 设置中添加当前地址，使用该客户端支持的 OAuth / DCR / PKCE 流程。")}</p><p>{t("保留能用的本地连接和正在运行的 Codex。按本地客户端的实际界面操作，不需要为这一步创建云端插件。")}</p><p className="assistant-guide-notice">{t("如果还要连接 dot，请另走云端路线，不要在管理页和桌面认证间重复跳转。")}</p></>)}
     {step===3&&<><p>{t("本人完成登录，核对当前实例和授权范围，再同意连接。")}</p><div className="assistant-guide-flow"><span>Agbrio</span><ArrowRight size={18}/><span>{t("本人登录与同意")}</span><ArrowRight size={18}/><span>{target}</span></div><p>{t("已有有效登录直接复用；浏览器确实需要登录时才使用现有配对入口。不要把手机配对码或兑换码当作 MCP token。")}</p><p className="assistant-guide-notice">{t("登录、范围核对和同意由你完成，AI 助手不能代点同意。")}</p></>}
     {step===4&&<><p>{t("在目标助手实际发现工具，再让它执行这段只读检查。不会向真实对话发送测试消息。")}</p><div className="assistant-guide-prompt"><CopyAction text={assistantReadOnlyInstructions(route,getLanguage())} label={t("复制只读验收指令")}/><pre>{assistantReadOnlyInstructions(route,getLanguage())}</pre></div><ol className="assistant-guide-evidence">{["服务可达","OAuth 完成","目标助手获得工具","只读验收通过"].map(label=><li key={label}><span>{t(label)}</span><small>{t("待实际核对")}</small></li>)}</ol><p className="assistant-guide-notice">{t("步骤进度只是教程导航。网页能打开、注册返回 201、桌面 ready 或上传 ZIP，都不是 dot 接管成功。")}</p></>}
     {step===5&&<><p>{t("让助手先问：你希望我接管哪些 Bridge？")}</p><p>{t("选好后确认当前任务、精确绑定、交接方向、必须询问的情况和暂停条件，不需要再填写 Brief / Insight。")}</p><div className="assistant-guide-prompt"><CopyAction text={assistantTakeoverInstructions(getLanguage(),route)} label={t("复制协作指令")}/><pre>{assistantTakeoverInstructions(getLanguage(),route)}</pre></div><p className="assistant-guide-notice">{t(universal?"整个应用可访问不等于全部委托。先确认所选助手能被事件真正唤醒，再另行授权订阅；不支持时按需操作，不新增轮询任务。":"整个应用可访问，不代表所有 Bridge 都已委托。重新扫描插件工具和事件后，由 dot 创建授权范围内的订阅。连接本身不会自动开启事件。")}</p></>}
     {openError&&<p role="alert">{openError}</p>}
     <div className="assistant-guide-footer"><button type="button" disabled={step===0} onClick={()=>navigate(step-1)}><ArrowLeft size={16}/>{t("上一步")}</button><button type="button" className="r2-action" disabled={step===labels.length-1} onClick={()=>navigate(step+1)}>{t("下一步")}<ArrowRight size={16}/></button></div>
    </div>
    </details>
   </>}
   <details className="assistant-guide-troubleshoot"><summary>{t("连接遇到问题")}</summary><dl>
    <dt>{t("服务或发现")}</dt><dd>{t("GET /mcp 返回 405、匿名 POST 返回 401 / invalid_token，可能是正常保护，不要关闭认证。")}</dd>
    <dt>{t("注册或授权跳转")}</dt><dd>{t("若一直提示登录，在所选助手的 MCP / 连接器设置中找到 agbrio，完成本人登录与同意。若没有出现授权页，先核对当前入口和客户端是否支持 OAuth。")}</dd>
    <dt>{t("token、工具发现或实际调用")}</dt><dd>{t("核对实际失败阶段、状态码、有效期、撤销状态及精确资源地址。没有底层证据时标为未知，不只反复点验证。")}</dd>
   </dl><p>{t("诊断只分享版本、阶段、时间、固定路径和错误码，不分享 Cookie、token、密钥、授权码、PKCE verifier、完整 OAuth 查询网址或私人对话。")}</p><button type="button" onClick={()=>void external("AUTH_DOCUMENTATION")}><ExternalLink size={16}/>{t("查看官方认证说明")}</button><p className="assistant-guide-note">https://developers.openai.com/plugins/build/auth</p></details>
  </div>
 </section>;
}
