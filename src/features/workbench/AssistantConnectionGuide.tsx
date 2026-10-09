import {useId,useRef,useState} from "react";
import {ArrowLeft,ArrowRight,Check,ExternalLink,Link2,ShieldCheck} from "lucide-react";
import {t,getLanguage,useLanguage} from "../../i18n";
import {CopyAction} from "./NativeReaderActions";
import {assistantSetupInstructions,assistantReadOnlyInstructions,assistantTakeoverInstructions,type AssistantRoute} from "./assistantSetupInstructions";
import "../../styles/assistant-guide.css";

type ExistingGrant={id:string;scope?:string;approvalMode?:string;label:string;expiresAt:number;revokedAt:number|null};
export type AssistantHelpPage="CHATGPT_PLUGINS"|"AUTH_DOCUMENTATION";
export function AssistantConnectionGuide({mcpUrl,grants,onAddGrant,onRouteChange,openHelp}:{mcpUrl:string|null;grants:ExistingGrant[];onAddGrant:()=>void;onRouteChange:(route:AssistantRoute)=>void;openHelp:(page:AssistantHelpPage)=>Promise<void>}){
 useLanguage();const id=useId(),heading=useRef<HTMLHeadingElement>(null);
 const[route,setRoute]=useState<AssistantRoute|null>(null),[step,setStep]=useState(0),[openError,setOpenError]=useState("");
 const active=grants.filter(g=>g.scope==="INSTANCE"&&g.approvalMode==="CONVERSATION_REVIEW"&&!g.revokedAt&&g.expiresAt>Date.now());
 let https=false;try{const u=new URL(mcpUrl??"");https=u.protocol==="https:"&&!u.username&&!u.password;}catch{/* Unconfigured address is not a connection. */}
 const labels=["检查入口","准备授权",route==="local"?"添加到 Codex":"注册云端插件","本人同意","只读验收","选择 Bridge"];
 function choose(value:AssistantRoute){setRoute(value);setStep(0);setOpenError("");onRouteChange(value);}
 function navigate(value:number){setStep(value);setOpenError("");requestAnimationFrame(()=>heading.current?.focus({preventScroll:true}));}
 async function external(page:AssistantHelpPage){setOpenError("");try{await openHelp(page);}catch{setOpenError(t("未能打开浏览器，请复制下方官方网址，在浏览器中打开。"));}}
 return <section className="r2-assistant-tutorial" aria-labelledby={`${id}-title`}>
  <h2 id={`${id}-title`} className="r2-sr-only">{t("连接你的助手")}</h2>
  <div id={`${id}-body`}>
   <p className="r2-sr-only">{t("你要连接哪类助手？")}</p>
   <div className="assistant-route-options" role="group" aria-label={t("助手类型")}>
    <button type="button" aria-pressed={route==="cloud"} onClick={()=>choose("cloud")}><span>ChatGPT / dot {t("云端")}</span></button>
    <button type="button" aria-pressed={route==="local"} onClick={()=>choose("local")}><span>Codex {t("本地")}</span></button>
   </div>
   {!route?<p className="assistant-route-hint">{t("两条路线分别连接。桌面端认证成功，不代表 dot 云端已经可用。")}</p>:<>
    <div className="assistant-agent-start"><CopyAction showLabel text={assistantSetupInstructions(mcpUrl,getLanguage(),route)} label={t("复制给 agent 的指令")}/><p>{t("检查现有部署、完成连接、排错和只读验收。")}</p></div>
    <div className="assistant-current-connection"><p>{t("当前 MCP 地址")}</p>{mcpUrl?<div className="assistant-guide-value"><code>{mcpUrl}</code><CopyAction text={mcpUrl} label={t("复制 MCP 地址")}/></div>:<p>{t("尚未配置 MCP 入口。先到设备设置选择已有 HTTPS 服务或部署方式。")}</p>}{active.length>0&&<p className="assistant-grant-summary"><Check size={18}/>{t("有效授权将直接复用 · 可随时撤销")}</p>}</div>
    <details className="assistant-human-details"><summary>{t("查看我需要做的步骤")}</summary><section className="assistant-human-steps" aria-label={t("只需要你完成")}><h3>{t("只需要你完成")}</h3>
     <ol><li><strong>{t("选择助手授权")}</strong>{active.length?<p>{t("已有有效的整个应用授权，可直接复用。")}</p>:<button type="button" onClick={onAddGrant}>{t("创建 30 天授权")}</button>}<p>{t("旧版单 Bridge 授权不会自动升级。")}</p></li>
     <li><strong>{t("本人登录并同意连接")}</strong>{route==="cloud"&&<button type="button" onClick={()=>void external("CHATGPT_PLUGINS")}>{t("在浏览器打开插件首页")}</button>}<p>{t(route==="cloud"?"按 agent 指引添加云端连接，核对范围并同意，再在目标 dot 中启用。":"按 agent 指引连接本地 Codex，核对范围并同意。")}</p></li>
     <li><strong>{t("告诉助手接管哪些 Bridge")}</strong><p>{t("说明任务、需要询问的情况和暂停条件；不需要另填 Brief。")}</p></li></ol>
    </section></details>
    {openError&&<p role="alert">{openError}</p>}
    <details className="assistant-prompt-preview"><summary>{t("预览要复制的指令")}</summary><pre>{assistantSetupInstructions(mcpUrl,getLanguage(),route)}</pre></details>
    <details className="assistant-manual"><summary>{t("自己操作：查看详细教程")}</summary>
    <nav className="assistant-guide-steps" aria-label={t("连接教程步骤")}>{labels.map((label,index)=><button key={index} type="button" aria-current={index===step?"step":undefined} aria-controls={`${id}-step`} onClick={()=>navigate(index)}><span className="assistant-step-number">{index+1}</span><span>{t(label)}</span></button>)}</nav>
    <div className="assistant-guide-content" id={`${id}-step`}>
     <p className="assistant-guide-count">{t("步骤 {0} / {1}",step+1,labels.length)}</p><h3 tabIndex={-1} ref={heading}>{t(labels[step])}</h3>
     {step===0&&<><p>{t(route==="cloud"?"先复用现有部署。云端助手需要能访问这个完整 HTTPS MCP 地址。":"使用当前实例的实际 MCP 地址。在 Codex 本地完成的认证，只证明这条本地连接。")}</p>{mcpUrl?<div className="assistant-guide-value"><Link2 size={18}/><code>{mcpUrl}</code><CopyAction text={mcpUrl} label={t("复制 MCP 地址")}/></div>:<p className="assistant-guide-notice">{t("尚未配置 MCP 入口。先到设备设置选择已有 HTTPS 服务或部署方式。")}</p>}<p className="assistant-guide-notice">{t(route==="cloud"&&mcpUrl&&!https?"当前地址不是 HTTPS，不能标为云端可达。":"已有可用入口时，不需要重新部署、重启或配对。")}</p><p className="assistant-guide-note">{t("地址来自你的实例配置；已配置地址不等于网络可达。")}</p></>}
     {step===1&&<><p>{t("选择现有有效的整个应用授权；没有时，再由本人添加。")}</p>{active.length?<div className="assistant-guide-existing"><ShieldCheck size={20}/><div><strong>{t("可复用的整个应用授权")}</strong>{active.map(g=><p key={g.id}>{g.label}<span>{t("至")} {new Date(g.expiresAt).toLocaleDateString(getLanguage())}</span></p>)}</div></div>:<button type="button" className="r2-action" onClick={onAddGrant}>{t("创建 30 天授权")}</button>}<p>{t("范围为 INSTANCE，会话审阅模式，可随时撤销。旧版单 Bridge 授权不会自动升级。")}</p><p className="assistant-guide-notice">{t("授权允许访问；还需要在目标助手完成 OAuth 连接。")}</p></>}
     {step===2&&(route==="cloud"?<><p>{t("在真正的浏览器打开 ChatGPT 插件首页，选择添加自定义 MCP 服务器。")}</p><button type="button" className="r2-action" onClick={()=>void external("CHATGPT_PLUGINS")}><ExternalLink size={17}/>{t("在浏览器打开插件首页")}</button><div className="assistant-guide-fields"><p><span>{t("服务器地址")}</span><code>{mcpUrl??t("从步骤 1 复制")}</code></p><p><span>{t("认证方式")}</span><strong>OAuth · DCR</strong></p><p><span>{t("请求范围")}</span><code>agbrio:instance</code></p><p><span>{t("客户端密钥")}</span><strong>{t("不填写")}</strong></p></div><p>{t("其他 OAuth 端点自动发现。创建插件后，安装并在目标 dot 中启用。")}</p><p className="assistant-guide-notice">{t("仅限桌面端的插件不会自动成为云端连接；保留它，再注册云端插件。")}</p><p className="assistant-guide-note">https://chatgpt.com/plugins</p></>:<><p>{t("在 Codex 的插件或 MCP 设置中添加当前地址，使用该客户端支持的 OAuth / DCR / PKCE 流程。")}</p><p>{t("保留能用的本地连接和正在运行的 Codex。按本地客户端的实际界面操作，不需要为这一步创建云端插件。")}</p><p className="assistant-guide-notice">{t("如果还要连接 dot，请另走云端路线，不要在管理页和桌面认证间重复跳转。")}</p></>)}
     {step===3&&<><p>{t("本人完成登录，核对当前实例和授权范围，再同意连接。")}</p><div className="assistant-guide-flow"><span>Agbrio</span><ArrowRight size={18}/><span>{t("本人登录与同意")}</span><ArrowRight size={18}/><span>{route==="cloud"?"dot":"Codex"}</span></div><p>{t("已有有效登录直接复用；浏览器确实需要登录时才使用现有配对入口。不要把手机配对码或兑换码当作 MCP token。")}</p><p className="assistant-guide-notice">{t("登录、范围核对和同意由你完成，AI 助手不能代点同意。")}</p></>}
     {step===4&&<><p>{t("在目标助手实际发现工具，再让它执行这段只读检查。不会向真实对话发送测试消息。")}</p><div className="assistant-guide-prompt"><CopyAction text={assistantReadOnlyInstructions(route,getLanguage())} label={t("复制只读验收指令")}/><pre>{assistantReadOnlyInstructions(route,getLanguage())}</pre></div><ol className="assistant-guide-evidence">{["服务可达","OAuth 完成","目标助手获得工具","只读验收通过"].map(label=><li key={label}><span>{t(label)}</span><small>{t("待实际核对")}</small></li>)}</ol><p className="assistant-guide-notice">{t("步骤进度只是教程导航。网页能打开、注册返回 201、桌面 ready 或上传 ZIP，都不是 dot 接管成功。")}</p></>}
     {step===5&&<><p>{t("让助手先问：你希望我接管哪些 Bridge？")}</p><p>{t("选好后确认当前任务、精确绑定、交接方向、必须询问的情况和暂停条件，不需要再填写 Brief / Insight。")}</p><div className="assistant-guide-prompt"><CopyAction text={assistantTakeoverInstructions(getLanguage())} label={t("复制协作指令")}/><pre>{assistantTakeoverInstructions(getLanguage())}</pre></div><p className="assistant-guide-notice">{t("整个应用可访问，不代表所有 Bridge 都已委托。事件订阅和自动唤醒尚未实现。")}</p></>}
     {openError&&<p role="alert">{openError}</p>}
     <div className="assistant-guide-footer"><button type="button" disabled={step===0} onClick={()=>navigate(step-1)}><ArrowLeft size={16}/>{t("上一步")}</button><button type="button" className="r2-action" disabled={step===labels.length-1} onClick={()=>navigate(step+1)}>{t("下一步")}<ArrowRight size={16}/></button></div>
    </div>
    </details>
   </>}
   <details className="assistant-guide-troubleshoot"><summary>{t("连接遇到问题")}</summary><dl>
    <dt>{t("服务或发现")}</dt><dd>{t("GET /mcp 返回 405、匿名 POST 返回 401 / invalid_token，可能是正常保护，不要关闭认证。")}</dd>
    <dt>{t("注册或授权跳转")}</dt><dd>{t("0.1.5 修复 SDK 注册 grant_types 兼容；0.1.6 修复多 scope 授权请求。请求多个 scope 不会扩大本人所选授权。")}</dd>
    <dt>{t("token、工具发现或实际调用")}</dt><dd>{t("核对实际失败阶段、状态码、有效期、撤销状态及精确资源地址。没有底层证据时标为未知，不只反复点验证。")}</dd>
   </dl><p>{t("诊断只分享版本、阶段、时间、固定路径和错误码，不分享 Cookie、token、密钥、授权码、PKCE verifier、完整 OAuth 查询网址或私人对话。")}</p><button type="button" onClick={()=>void external("AUTH_DOCUMENTATION")}><ExternalLink size={16}/>{t("查看官方认证说明")}</button><p className="assistant-guide-note">https://developers.openai.com/plugins/build/auth</p></details>
  </div>
 </section>;
}
