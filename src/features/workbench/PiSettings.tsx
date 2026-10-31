import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {t,useLanguage} from '../../i18n';

type Workspace={roots:string[];machineScope:boolean;writable:boolean;terminal:boolean;administrator:boolean;inheritContext:boolean;grantIds:string[]};
type Scope={id:string;root:string;generation:number;writable:boolean;enabled:boolean;policy?:Workspace|null};
type View={administratorBackend?:{ready:boolean;preparing?:boolean};installed:boolean;version:string|null;scopes:Scope[];assistantGrants?:{id:string;label:string}[]};
export const piSettingsApi={prepareAdministrator:()=>invoke<View>('pi_prepare_administrator'),authorizeWorkspace:(input:Workspace)=>invoke<View>('pi_authorize_workspace',{input}),read:()=>invoke<View>('pi_settings'),authorize:(root:string,writable:boolean)=>invoke<View>('pi_authorize_directory',{root,writable}),revoke:(scopeId:string,generation:number)=>invoke<View>('pi_revoke_directory',{scopeId,generation})};
export function PiSettings({api=piSettingsApi}:{api?:Omit<typeof piSettingsApi,'authorizeWorkspace'|'prepareAdministrator'>&{authorizeWorkspace?:typeof piSettingsApi.authorizeWorkspace;prepareAdministrator?:typeof piSettingsApi.prepareAdministrator}}){
 useLanguage();const [view,setView]=useState<View|null>(null),[root,setRoot]=useState(''),[writable,setWritable]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('');const mounted=useRef(true),pending=useRef(false),seeded=useRef(false);
 const [machineScope,setMachineScope]=useState(true),[workspaceWrite,setWorkspaceWrite]=useState(true),[terminal,setTerminal]=useState(true),[inheritContext,setInheritContext]=useState(true),[grantIds,setGrantIds]=useState<string[]>([]);
 useEffect(()=>{mounted.current=true;void api.read().then(v=>{if(mounted.current){setView(v);if(!seeded.current){seeded.current=true;if(v.assistantGrants?.length===1)setGrantIds([v.assistantGrants[0].id]);}}}).catch(()=>{if(mounted.current)setError(t('暂时无法读取 Pi 设置，请重试。'));});return()=>{mounted.current=false;};},[api]);
 async function act(work:()=>Promise<View>,success=''){if(pending.current)return;pending.current=true;setBusy(true);setError('');setNotice('');try{const next=await work();if(mounted.current){setView(next);setNotice(success);}}catch(reason){if(mounted.current){const code=String(reason);setError(t(code.includes('UAC_CANCELLED')?'已取消 Windows 授权，未启动管理员后台。':code.includes('ADMIN_BACKEND')?'管理员后台尚未就绪，请在电脑上点击“准备管理员后台”并确认 Windows 弹窗。':code.includes('IN_USE')?'该目录仍有 Pi 对话打开，请先停止这些 Pi 任务，再调整目录权限。':code.includes('PROTECTED')||code.includes('TOO_BROAD')?'请选择具体项目文件夹，不能授权整个磁盘、用户目录或系统目录。':code.includes('LINK')?'这个路径经过链接或映射目录，请填写项目的真实本地路径。':code.includes('DIRECTORY')?'未找到可用的本地项目文件夹，请检查路径后重试。':'未能保存 Pi 设置，请重试。'));}}finally{pending.current=false;if(mounted.current)setBusy(false);}}
 const scopes=view?.scopes.filter(s=>s.enabled)??[];
 return <section className="v4-shared-codex pi-settings" aria-label={t('Pi 执行对话')}>
  <h2>{t('Pi 执行对话')}</h2><p>{t('让管理对话把独立任务交给 Pi，在 Agbrio 查看进度、跟进和读取结果。')}</p>
  {!view&&!error&&<p role="status">{t('正在读取本机 Pi…')}</p>}
  {view&&<><p className="v4-meta">{view.installed?t('已安装 Pi {0}',view.version??''):t('本机尚未安装 Pi Agent。安装并配置好 Pi 后，回来刷新状态。')}</p>
   {view.installed&&<>{api.prepareAdministrator&&<div className="pi-administrator-setup"><h3>{t('管理员执行后台')}</h3><p>{t(view.administratorBackend?.ready?'管理员后台已就绪，新 Pi 子任务会直接复用。':'在这台电脑确认一次 Windows UAC，随后手机和管理助手派单可直接复用。重启 Agbrio 后需要重新准备。')}</p>{!view.administratorBackend?.ready&&<button type="button" className="v3-primary" disabled={busy} onClick={()=>void act(api.prepareAdministrator!,t('管理员后台已就绪。'))}>{t(busy?'请在 Windows 弹窗中确认…':'准备管理员后台')}</button>}</div>}<h3>{t('管理助手的工作范围')}</h3><p>{t('在这里授权一次。管理助手随后为每个 Pi 子任务选择工作目录和可访问的文件夹。')}</p>
    {scopes.length>0?<ul className="pi-scope-list">{scopes.map(scope=><li key={scope.id}><div><strong className="pi-directory-path">{scope.policy?.machineScope?t('本机项目工作区'):scope.policy?.roots.join(' · ')??scope.root}</strong>{scope.policy?.machineScope&&<small>{t('已授权磁盘：')}{scope.policy.roots.join(' · ')}</small>}{scope.policy&&<small>{t(scope.policy.administrator?'管理员执行 · Windows UAC':'普通权限执行')}{scope.policy.terminal?' · '+t('可执行命令'):''}</small>}<small>{t(scope.writable?'可读取与修改':'只读')}</small></div><button type="button" disabled={busy} onClick={()=>void act(()=>api.revoke(scope.id,scope.generation),t('目录权限已撤销，相关 Pi 任务已停止。'))}>{t('撤销并停止')}</button></li>)}</ul>:<p className="v4-meta">{t('还没有授权目录。先添加一个项目文件夹，管理对话才能创建 Pi 子任务。')}</p>}
    {api.authorizeWorkspace?<form onSubmit={event=>{event.preventDefault();void act(()=>api.authorizeWorkspace!({roots:root.split(/\r?\n/).map(v=>v.trim()).filter(Boolean),machineScope,writable:workspaceWrite,terminal,administrator:true,inheritContext,grantIds}),t('工作范围已保存，管理助手可以为子任务分配目录。'));}}>
     <label className="pi-write-choice"><input type="checkbox" checked={machineScope} disabled={busy} onChange={e=>setMachineScope(e.target.checked)}/>{t('允许管理助手选择本机项目目录')}</label>
     {!machineScope&&<><label htmlFor="pi-workspace-roots">{t('可访问的文件夹（每行一个）')}</label><textarea id="pi-workspace-roots" value={root} onChange={e=>setRoot(e.target.value)} disabled={busy} rows={3} spellCheck={false}/></>}
     <fieldset><legend>{t('允许使用的助手连接')}</legend>{view.assistantGrants?.length?view.assistantGrants.map(grant=><label className="pi-write-choice" key={grant.id}><input type="checkbox" checked={grantIds.includes(grant.id)} disabled={busy} onChange={e=>setGrantIds(old=>e.target.checked?[...old,grant.id]:old.filter(id=>id!==grant.id))}/>{grant.label}</label>):<p>{t('请先在 AI 助手中添加一个连接。')}</p>}</fieldset>
     <label className="pi-write-choice"><input type="checkbox" checked={workspaceWrite} disabled={busy} onChange={e=>{setWorkspaceWrite(e.target.checked);if(!e.target.checked)setTerminal(false);}}/>{t('允许读写项目文件')}</label>
     <label className="pi-write-choice"><input type="checkbox" checked={terminal} disabled={busy||!workspaceWrite} onChange={e=>setTerminal(e.target.checked)}/>{t('允许执行任务所需的终端命令')}</label>
     <label className="pi-write-choice"><input type="checkbox" checked={inheritContext} disabled={busy} onChange={e=>setInheritContext(e.target.checked)}/>{t('加载全局与项目规则、skills')}</label>
     <p className="v4-meta">{t('新 Pi 任务复用电脑上已准备好的管理员后台。终端拥有本机账户权限；文件夹范围用于派单和文件工具，不是系统沙箱。')}</p>
     <button type="submit" className="v3-primary" disabled={busy||grantIds.length===0||!machineScope&&!root.trim()}>{t(busy?'正在处理…':'授权工作范围')}</button>
    </form>:<form onSubmit={event=>{event.preventDefault();void act(()=>api.authorize(root.trim(),writable),t('目录已授权，管理对话可以按现有审批规则派发任务。'));}}>
     <label htmlFor="pi-project-directory">{t('项目文件夹')}</label><input id="pi-project-directory" value={root} disabled={busy} onChange={e=>setRoot(e.target.value)} placeholder="D:\Projects\example" autoComplete="off" spellCheck={false}/>
     <label className="pi-write-choice"><input type="checkbox" checked={writable} disabled={busy} onChange={e=>setWritable(e.target.checked)}/>{t('允许修改此目录内的项目文件')}</label>
     <button type="submit" disabled={busy||!root.trim()}>{t(busy?'正在处理…':'授权目录')}</button>
    </form>}<p className="v4-meta">{t('目录授权只在这台电脑设置。Pi 使用自己的 API 配置；订阅账号、其他对话和原客户端权限保持各自独立。')}</p>
   </>}
  </>}
  <button type="button" disabled={busy} onClick={()=>void act(api.read)}>{t('刷新状态')}</button>
  {notice&&<p role="status">{notice}</p>}{error&&<p role="alert">{error}</p>}
 </section>;
}
