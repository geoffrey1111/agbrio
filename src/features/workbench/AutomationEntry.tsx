import {useEffect,useState} from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {Link2,Terminal,X} from 'lucide-react';
import {t,useLanguage} from '../../i18n';
import {request} from '../../mobile/api';
import {assistantMcpConfig} from './assistantClientInstructions';
import {CopyAction} from './NativeReaderActions';
import '../../styles/automation-entry.css';

type EntryApi={mcp:()=>Promise<string|null>;cli?:()=>Promise<string|null>};
const desktop:EntryApi={
 mcp:()=>invoke<{mcpUrl:string|null}>('assistant_settings').then(value=>value.mcpUrl),
 cli:()=>invoke<{localMaintenance:{cliPath:string|null}}>('app_capabilities').then(value=>value.localMaintenance.cliPath),
};
const phone:EntryApi={mcp:()=>request<{mcpUrl:string|null}>('/assistant/settings').then(value=>value.mcpUrl)};

export function AutomationEntry({onConfigure,api=isTauri()?desktop:phone}:{onConfigure?:()=>void;api?:EntryApi}){
 useLanguage();
 const [open,setOpen]=useState(false),[endpoint,setEndpoint]=useState<string|null|undefined>(),[cli,setCli]=useState<string|null|undefined>();
 useEffect(()=>{
  if(!open)return;
  let current=true;
  void api.mcp().then(url=>{if(current)setEndpoint(assistantMcpConfig(url)?url:null);},()=>{if(current)setEndpoint(null);});
  void api.cli?.().then(path=>{if(current)setCli(path??null);},()=>{if(current)setCli(null);});
  return()=>{current=false;};
 },[api,open]);
 const prompt=endpoint?t('请为当前客户端（Codex 或 Claude Code）连接 Agbrio MCP：服务名 agbrio，Streamable HTTP 地址 {0}，使用 OAuth。先检查并复用已有配置，保留其他连接和授权；引导我完成登录和同意。连接后运行 agbrio_capabilities，列出实际授权的能力，不发送测试消息。若需要重连或重启客户端，请先告知我。',endpoint):null;
 const command=cli?`pwsh -NoProfile -File '${cli.replace(/'/g,"''")}' -Command Capabilities`:null;
 return <Dialog.Root open={open} onOpenChange={next=>{setOpen(next);if(next){setEndpoint(undefined);setCli(undefined);}}}>
  <Dialog.Trigger asChild><button type="button" className="agbrio-automation-trigger"><Terminal size={19} aria-hidden/>MCP &amp; CLI</button></Dialog.Trigger>
  <Dialog.Portal>
   <Dialog.Overlay className="v3-role-dialog-overlay"/>
   <Dialog.Content className="agbrio-automation-dialog">
    <header><Dialog.Title>Agbrio MCP &amp; CLI</Dialog.Title><Dialog.Close asChild><button type="button" className="agbrio-automation-close" aria-label={t('关闭')}><X size={20} aria-hidden/></button></Dialog.Close></header>
    <Dialog.Description>{t('连接 AI 助手用 MCP，本机操作用 CLI。')}</Dialog.Description>
    <section>
     <h3><Link2 size={20} aria-hidden/>Agbrio MCP</h3>
     <p>{t('复制提示词，发给 Codex 或 Claude Code，即可开始配置。')}</p>
     {prompt?<><pre>{prompt}</pre><CopyAction text={prompt} label={t('复制提示词')} showLabel/></>:endpoint===undefined?<p role="status">{t('正在读取连接状态…')}</p>:<div className="agbrio-automation-fallback"><p>{t('暂时没有可用的 MCP 地址。请在 AI 助手设置检查连接。')}</p>{onConfigure&&<button type="button" onClick={()=>{setOpen(false);onConfigure();}}>{t('打开 AI 助手设置')}</button>}</div>}
    </section>
    <section>
     <h3><Terminal size={20} aria-hidden/>Agbrio CLI</h3>
     {api.cli?<><p>{t('在运行 Agbrio 的 Windows 电脑上，用 PowerShell 7 执行以下命令，查看可用操作。')}</p>
     {command?<div className="agbrio-automation-command"><pre><code>{command}</code></pre><CopyAction text={command} label={t('复制命令')}/></div>:<p role="status">{t(cli===undefined?'正在读取…':'暂时无法读取本机 CLI 路径。')}</p>}</>:<p>{t('请到运行 Agbrio 的 Windows 电脑上，打开侧边栏的 MCP & CLI 获取本机命令。')}</p>}
    </section>
   </Dialog.Content>
  </Dialog.Portal>
 </Dialog.Root>;
}
