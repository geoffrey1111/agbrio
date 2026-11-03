import {useRef,useState,type ReactNode} from 'react';
import {ArrowLeft,ArrowRight,Check,ExternalLink} from 'lucide-react';
import {t,getLanguage,useLanguage} from '../../i18n';
import {CopyAction} from './NativeReaderActions';
import {ClaudeNativeConnection} from './ClaudeNativeConnection';
import type {ClaudeConnectionResult} from './ClaudeSessionPicker';
import {claudeCodeAddCommand,claudeManualConnectionInstructions} from './claudeConnectionInstructions';
import {assistantMcpConfig} from './assistantClientInstructions';
import {assistantReadOnlyInstructions} from './assistantSetupInstructions';
import {mcpAddressDiscoveryPowerShell} from './claudeConnectionRecovery';
import {AssistantDailyReport,type ReportBridge} from './AssistantDailyReport';

export function ClaudeAssistantSetup({mcpUrl,hasGrant,onAddGrant,grantForm,onRefresh,onConfigureConnection,openConnectors,connectClaude,bridges=[]}:{mcpUrl:string|null;hasGrant:boolean;onAddGrant:()=>void;grantForm?:ReactNode;onRefresh?:()=>Promise<void>;onConfigureConnection?:()=>void;openConnectors:()=>Promise<void>;connectClaude?:()=>Promise<ClaudeConnectionResult>;bridges?:ReportBridge[]}){
 useLanguage();const [step,setStep]=useState(0),[client,setClient]=useState<'desktop'|'terminal'>('desktop'),[error,setError]=useState(''),[refreshing,setRefreshing]=useState(false);const heading=useRef<HTMLHeadingElement>(null),refreshPending=useRef(false);
 const url=assistantMcpConfig(mcpUrl)?mcpUrl:null,labels=['准备连接','添加并授权','验证工具'];
 function navigate(next:number){setStep(next);setError('');requestAnimationFrame(()=>heading.current?.focus());}
 async function refresh(){if(!onRefresh||refreshPending.current)return;refreshPending.current=true;setRefreshing(true);try{await onRefresh();}catch{setError(t('连接信息读取失败，请稍后重试。'));}finally{refreshPending.current=false;setRefreshing(false);}}
 async function open(){setError('');try{await openConnectors();}catch{setError(t('未能打开浏览器，请复制下方官方网址，在浏览器中打开。'));}}
 const address=<section className="assistant-setup-address" aria-label={t('当前 MCP 地址')}><span>{t('当前 MCP 地址')}</span>{url?<div><code>{url}</code><CopyAction text={url} label={t('复制 MCP 地址')}/></div>:<p>{t('尚未取得连接地址')}</p>}</section>;
 return <section className="assistant-setup" aria-label={t('Claude Code 完整接入')}>
  <header><h2>{t('连接 Claude Code')}</h2><p>{t('让 Claude 查看电脑上的进度、创建 Codex 对话并跟进任务。')}</p></header>
  <nav className="assistant-setup-steps" aria-label={t('Claude 连接步骤')}>{labels.map((label,index)=><button key={label} type="button" aria-current={step===index?'step':undefined} onClick={()=>navigate(index)}><span>{index+1}</span>{t(["准备","添加","验证"][index])}</button>)}</nav>
  <div className="assistant-setup-panel">
   <h3 ref={heading} tabIndex={-1}>{t(labels[step])}</h3>
   {step===0&&<>
    {address}
    {!url?<div role="status"><p>{t('先准备真实连接地址，暂时不能复制接入指令。')}</p><p>{t('手机已经能连接这台电脑？刷新即可尝试取得同一个连接地址。')}</p><button className="r2-action" type="button" disabled={refreshing||!onRefresh} onClick={()=>void refresh()}>{t(refreshing?'正在读取…':'刷新连接信息')}</button>{onConfigureConnection&&<button type="button" onClick={onConfigureConnection}>{t('打开设备连接设置')}</button>}<details><summary>{t('仍然没有地址？')}</summary><p>{t('先在设备设置完成手机连接，再回到这里刷新。也可让本机助手只读检查地址。')}</p><CopyAction showLabel text={mcpAddressDiscoveryPowerShell} label={t('复制只读查址命令')}/></details></div>:<p className="assistant-setup-status"><Check size={16} aria-hidden/>{t('连接地址已准备好')}</p>}
    <div className="assistant-setup-access"><h4>{t('允许助手访问 Agbrio')}</h4>{hasGrant?<p className="assistant-setup-status"><Check size={16} aria-hidden/>{t('已有有效授权，可直接复用')}</p>:<><p>{t('先添加一份可随时撤销的 30 天授权，再到 Claude 中确认连接。')}</p>{grantForm??<button type="button" disabled={!url} onClick={onAddGrant}>{t('创建 30 天授权')}</button>}</>}</div>
    <p className="assistant-setup-note">{t('地址和授权准备好后，仍需在实际使用的 Claude 对话中添加连接。')}</p>
   </>}
   {step===1&&<>
    <div className="assistant-setup-clients" role="group" aria-label={t('Claude 使用方式')}><button type="button" aria-pressed={client==='desktop'} onClick={()=>setClient('desktop')}>{t('Claude 桌面版 Code')}</button><button type="button" aria-pressed={client==='terminal'} onClick={()=>setClient('terminal')}>{t('终端 / VS Code')}</button></div>
    {client==='desktop'?<ol className="assistant-setup-actions">
     <li><strong>{t('在浏览器打开 Claude 连接器设置')}</strong><p>{t('点击下方按钮，在浏览器登录你使用 Claude 桌面版的同一个账号。然后点击 + Add → Add custom connector。')}</p><button type="button" className="r2-action" onClick={()=>void open()}><ExternalLink size={16}/>{t('打开 Claude 连接器设置')}</button><small>{t('也可自己打开 Claude 的 Customize → Connectors；旧版入口为 Settings → Connectors。')}</small></li>
     <li><strong>{t('复制下面两项，填进刚打开的添加窗口')}</strong><p>{t('分别点击复制，再切回 Claude 的 Add custom connector 窗口，粘贴到对应输入框。')}</p>
      <dl className="assistant-setup-fields"><div><dt>{t('名称 · Name')}</dt><dd><code>agbrio</code><CopyAction showLabel text="agbrio" label={t('复制名称')}/></dd></div><div><dt>{t('MCP 服务器地址 · URL')}</dt><dd><code>{url??t('尚未取得连接地址')}</code><CopyAction showLabel text={url} label={t('复制 MCP 地址')}/></dd></div></dl>
      <small>{t('使用 OAuth；若出现 OAuth client 选项，选 Register automatically，其余凭据留空。')}</small>
     </li>
     <li><strong>{t('保存连接，并在 Agbrio 页面允许访问')}</strong><p>{t('在 Claude 点击 Add 保存；若显示 Connect，再点 Connect。跳转到 Agbrio 后，选择第一步准备的授权并点击允许。已有有效登录可以复用。')}</p><details><summary>{t('授权页要求配对，或没有可选授权？')}</summary><p>{t('要求配对时，到电脑 Agbrio 的“设置 → 设备”获取配对码；没有可选授权时，返回第一步创建授权，再回来继续。')}</p></details></li>
     <li><strong>{t('回到 Claude 桌面版，在管理对话中启用')}</strong><p>{t('打开你要用来管理任务的那条 Code 对话。点击输入框旁的 + → Connectors，打开 agbrio，然后继续下面的验证。')}</p></li>
    </ol>:<><p>{t('下面的命令都在使用 Claude Code 的这台电脑上操作。')}</p><ol className="assistant-setup-actions"><li><strong>{t('在 PowerShell 检查已有连接')}</strong><CopyAction showLabel text="claude mcp get agbrio" label={t('复制检查命令')}/><code>claude mcp get agbrio</code><p>{t('复制后粘贴到 PowerShell，按回车执行。已存在且地址正确时直接复用。')}</p></li><li><strong>{t('没有连接时，再添加 agbrio')}</strong><CopyAction showLabel text={claudeCodeAddCommand(url)} label={t('复制添加命令')}/><p>{t('同样粘贴到 PowerShell 并执行，然后回到你要管理任务的 Claude Code 会话。')}</p><details><summary>{t('查看添加命令')}</summary><pre>{claudeCodeAddCommand(url)??t('请先完成准备连接')}</pre></details></li><li><strong>{t('在目标会话登录')}</strong><p>{t('在 Claude Code 的消息输入框输入 /mcp，选择 agbrio 完成登录，然后继续下一步验证。')}</p></li></ol></>}

    {(!url||!hasGrant)&&<p role="status">{t('连接地址或授权尚未准备好，请返回第一步。')}</p>}
    <details><summary>{t('复制完整手动步骤')}</summary><CopyAction showLabel text={url?claudeManualConnectionInstructions(url,getLanguage()):null} label={t('复制手动添加步骤')}/></details>
   </>}
   {step===2&&<>
    <p>{t('回到刚才启用 agbrio 的那条 Claude Code 管理对话。点击下面的复制按钮，把指令粘贴到这条对话的消息输入框，再点击发送。')}</p><CopyAction showLabel text={url?assistantReadOnlyInstructions('claude',getLanguage()):null} label={t('复制只读检查指令')}/><details><summary>{t('查看要发给 Claude 的检查内容')}</summary><pre>{assistantReadOnlyInstructions('claude',getLanguage())}</pre></details>
    <div className="assistant-setup-result"><h4>{t('看到什么才算连上？')}</h4><p>{t('Claude 实际调用 agbrio_read_app，并返回你的 Agbrio 状态和权限信息。')}</p><p>{t('只有 codex_wait、插件已安装或“指令已收到”，都还没有完成这一步。')}</p></div>
    <details><summary>{t('没有找到 agbrio 工具？')}</summary><p>{t('先回到第二步，确认当前 Code 对话的 Connectors 中已打开 agbrio；提示登录时先完成授权，再让 Claude 重新检查工具。')}</p><p>{t('MCP 连接查看连接器；通知插件列表不能代替这项检查。')}</p></details>
    <p>{t('读取成功后，直接告诉 Claude 要跟进哪些对话，以及哪些事需要先问你。')}</p>
   </>}
   {error&&<p role="alert">{error}</p>}
   <footer className="assistant-setup-footer">{step>0&&<button type="button" onClick={()=>navigate(step-1)}><ArrowLeft size={16}/>{t('上一步')}</button>}{step<2&&<button type="button" className="r2-action" disabled={step===0&&(!url||!hasGrant)} onClick={()=>navigate(step+1)}>{t(step===0?'去 Claude 添加连接':'查看验证方法')}<ArrowRight size={16}/></button>}</footer>
  </div>
  <details className="assistant-setup-extra"><summary>{t('接收完成通知与手机回复（可选）')}</summary><p>{t('MCP 读取和派发任务可以先使用。需要 Claude 接收提醒或手机回复时，再安装本机组件。')}</p><ClaudeNativeConnection connect={connectClaude} mcpUrl={url}/></details>
  <details className="assistant-setup-extra"><summary>{t('设置日报（可选）')}</summary><AssistantDailyReport bridges={bridges} mcpUrl={url}/></details>
 </section>;
}
