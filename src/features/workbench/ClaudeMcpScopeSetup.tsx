import {useState} from 'react';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {Check} from 'lucide-react';
import {t} from '../../i18n';
/** Configuration only. Never launches a model, prompts, changes an account or
 * treats the notification plugin as proof of MCP tool discovery. */
export function ClaudeMcpScopeSetup(){
 const [busy,setBusy]=useState(false),[configured,setConfigured]=useState(false),[error,setError]=useState('');
 if(!isTauri()||!navigator.userAgent.includes('Windows'))return null;
 async function configure(){
  setBusy(true);setError('');
  try{await invoke('claude_code_configure_mcp');setConfigured(true);}
  catch(e){setError(t(String(e).includes('DIFFERENT_SERVER_PRESERVED')?'已存在另一个 Agbrio 地址，已保留原配置。请核对后再连接。':'未能完成整机配置。请展开下方手动步骤，使用实际运行的 Claude Code 引擎检查。'));}
  finally{setBusy(false);}
 }
 return <section className="assistant-setup-access" aria-label={t('整机 MCP 配置')}><h4>{t('让不同项目的 Code 对话都能找到 Agbrio')}</h4><p>{t('点击一次，为当前 Windows 用户配置 MCP。原项目配置和已登录账号保留，不需要给每个项目重复添加。')}</p><button type="button" className="r2-action" disabled={busy} onClick={()=>void configure()}>{t(busy?'正在配置…':configured?'重新检查整机配置':'为这台电脑配置 MCP')}</button>{configured&&<><p className="assistant-setup-status" role="status"><Check size={16} aria-hidden/>{t('用户级配置已就绪，适用于不同项目。')}</p><p>{t('已经打开的 Code 对话可能仍在使用旧工具列表。请在原对话通过 /mcp 重新连接；找不到入口时，保存任务后重新打开原会话，不必删除对话或重新安装插件。随后执行下方只读验证。')}</p></>}{error&&<p role="alert">{error}</p>}</section>;
}
