import {PwaInstallGuide} from '../workbench/ConnectionGuides';
import type {ExistingCodexThreadCandidate} from '../codex/types';
import {useEffect,useMemo,useRef,useState,type ReactNode} from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import {ArrowLeft,ArrowRight,Check,Circle,Copy,LoaderCircle,MessagesSquare,Smartphone,Terminal} from 'lucide-react';
import qrcode from 'qrcode-generator';
import {t,getLanguage,useLanguage} from '../../i18n';
import {claudeManualConnectionInstructions} from '../workbench/claudeConnectionInstructions';
import {connectionError} from '../workbench/claudeConnectionErrors';
import type {FirstRunApi,OwnerAuthorizationRequest} from './firstRunApi';
import {codexReady,hostedRenewalRequired,nextSetupStep,phoneReady,remoteReady,setupPlan,setupStorage,writeSetupPreference,type SetupAgent,type SetupPreference,type SetupSnapshot,type SetupStep} from './firstRunModel';
import './first-run.css';

export function phoneUrl(origin?:string|null){try{const u=new URL(origin??'');return u.protocol==='https:'&&!u.username&&!u.password&&!u.search&&!u.hash&&u.pathname==='/'?new URL('/mobile',u.origin).href:null;}catch{return null;}}
function PhoneQr({url}:{url:string}){const image=useMemo(()=>{const qr=qrcode(0,'M');qr.addData(url);qr.make();const size=qr.getModuleCount();let path='';for(let y=0;y<size;y++)for(let x=0;x<size;x++)if(qr.isDark(y,x))path+=`M${x+4},${y+4}h1v1h-1z`;return {size:size+8,path};},[url]);return <svg className="first-run-qr" viewBox={`0 0 ${image.size} ${image.size}`} role="img" aria-label={t('手机扫码打开 Agbrio')}><rect width={image.size} height={image.size} fill="#fff"/><path d={image.path} fill="#111"/></svg>;}
/** Isolated preview only: explanatory copy; never a source of connection facts. */
export type FirstRunPreview={description?:(step:SetupStep)=>string|undefined};
export const READ_CHECK='请只调用当前 Agbrio MCP 的 agbrio_read_app，报告读取到的本机状态、scope、approvalMode 和有效期。不要创建对话或发送任务。';
export const STARTER='请用当前 Agbrio MCP 只读列出我电脑上的对话及状态，确认可以读取。先不要创建对话或发送业务消息；等我给出任务后，再按我的明确要求创建或跟进执行对话。';
const headings:Record<SetupStep,string>={CHOOSE:'你在电脑上使用哪些 Agent？',CODEX:'连接 Codex',PHONE:'连接你的手机',CLAUDE:'连接 Claude Code',WATCH:'选择要在手机上跟进的对话',VERIFY:'在手机完成第一次跟进',DONE:'可以开始使用了'};
const detail:Record<SetupStep,string>={CHOOSE:'只设置你选中的 Agent。已经完成的部分会自动跳过，不会重复安装。',CODEX:'让 Agbrio 和 Codex 共用同一组对话。点一次会先准备共享后台，再打开共享 Codex。出现 Windows 管理员确认时，请在系统弹窗中同意。',PHONE:'手机和 Claude 助手共用这个连接地址。已有连接会保留，开通试用需要你确认。',CLAUDE:'一次完成通知插件、30 天访问授权和本机 MCP 配置。浏览器里的“允许”由你本人点击；已有同名但地址不同的连接会保留，不会覆盖。',WATCH:'每个 Agent 选一条。监听后它会出现在手机的对话列表，可以查看并回复原对话。',VERIFY:'在手机 Agbrio 的「对话」页打开下面的对话，确认能看到最近消息；在空闲的测试对话里发一句你自己的话，确认电脑收到、手机也显示回复。',DONE:'现在可以从手机跟进对话。需要调整时，从设置重新打开引导。'};
type Row={label:string;state:'DONE'|'ACTIVE'|'TODO';note?:string};
function Checklist({rows}:{rows:Row[]}){return <ol className="first-run-checklist">{rows.map(row=><li key={row.label} data-state={row.state}>{row.state==='DONE'?<Check size={16} aria-hidden/>:row.state==='ACTIVE'?<LoaderCircle size={16} className="v4-progress-spinner" aria-hidden/>:<Circle size={14} aria-hidden/>}<span><strong>{t(row.label)}</strong>{row.note&&<small>{t(row.note)}</small>}</span><span className="v7-sr">{t(row.state==='DONE'?'已完成':row.state==='ACTIVE'?'进行中':'待完成')}</span></li>)}</ol>;}

export function FirstRunWizard({api,initial={},preference,storage=setupStorage,onExit,companion,preview}:{api:FirstRunApi;preview?:FirstRunPreview;companion?:(step:SetupStep)=>ReactNode;initial?:SetupSnapshot;preference?:SetupPreference|null;storage?:Pick<Storage,'setItem'>;onExit:(disposition:'SKIPPED'|'COMPLETED',agents:SetupAgent[])=>void}){
 useLanguage();
 const startAgents=preference?.agents??[];
 const [agents,setAgents]=useState<SetupAgent[]>(startAgents),[step,setStep]=useState<SetupStep>(preference&&setupPlan(startAgents).includes(preference.step)?preference.step:'CHOOSE'),[data,setData]=useState<SetupSnapshot>(initial),[busy,setBusy]=useState(false),[error,setError]=useState(''),[reviewRequired,setReviewRequired]=useState(!!preference?.needsCheck&&preference.needsCheck===preference.step);
 const [mode,setMode]=useState<'TRIAL'|'CODE'|'EXISTING'>('TRIAL'),[activation,setActivation]=useState(''),[origin,setOrigin]=useState(''),[pair,setPair]=useState<{code:string;expiresAt:number}|null>(null),[homeCode,setHomeCode]=useState<{code:string;expiresAt:number}|null>(null),[now,setNow]=useState(Date.now());
 const [codexAction,setCodexAction]=useState<'PREPARE'|'OPEN'|''>(''),[closeRequired,setCloseRequired]=useState(false);
 const [pluginInstalled,setPluginInstalled]=useState(false),[mcpPhase,setMcpPhase]=useState(''),[authRequests,setAuthRequests]=useState<OwnerAuthorizationRequest[]>([]),[authRequestId,setAuthRequestId]=useState(''),[openedAuthId,setOpenedAuthId]=useState(''),[authPair,setAuthPair]=useState<{code:string;expiresAt:number}|null>(null),[manualAuthorized,setManualAuthorized]=useState(false),[manualMcp,setManualMcp]=useState(false);
 const [verifyCopied,setVerifyCopied]=useState(false),[copyFailed,setCopyFailed]=useState<string>(''),[ownerVerified,setOwnerVerified]=useState(false),[starterCopied,setStarterCopied]=useState(false);
 const [candidates,setCandidates]=useState<ExistingCodexThreadCandidate[]|null>(null),[watching,setWatching]=useState<string[]>([]),[selection,setSelection]=useState<Partial<Record<SetupAgent,string>>>({}),[watchedNames,setWatchedNames]=useState<string[]>([]),[phoneVerified,setPhoneVerified]=useState(false),[listRevision,setListRevision]=useState(0);
 const pending=useRef(false),mounted=useRef(true),readRevision=useRef(0),latest=useRef(data),title=useRef<HTMLHeadingElement>(null),autoOpen=useRef(false),pairIssued=useRef(false);
 const plan=setupPlan(agents),mobile=phoneUrl(data.origin),pairExpired=!!pair&&pair.expiresAt<=now/1000,readyPhone=(data.devices?.length??0)>0,remote=!!mobile&&remoteReady(data);
 const mcpAuthorized=mcpPhase==='AUTHORIZED'||manualAuthorized,mcpStarted=!!mcpPhase&&mcpPhase!=='NOT_STARTED'||manualMcp;
 latest.current=data;
 function accept(next:SetupSnapshot){if(!mounted.current)return;setData(old=>({...old,...Object.fromEntries(Object.entries(next).filter(([,v])=>v!==undefined))}));}
 async function refresh(){const ticket=++readRevision.current;const next=await api.snapshot(agents);if(ticket!==readRevision.current)return latest.current;accept(next);return {...latest.current,...Object.fromEntries(Object.entries(next).filter(([,v])=>v!==undefined))} as SetupSnapshot;}
 function go(target:SetupStep){if(!mounted.current)return;setError('');setCopyFailed('');setReviewRequired(false);setCloseRequired(false);setStep(target);}
 function next(snapshot=latest.current){go(nextSetupStep(plan,step,snapshot));}
 useEffect(()=>{mounted.current=true;const ticket=++readRevision.current;void api.snapshot(agents).then(s=>{if(mounted.current&&ticket===readRevision.current)accept(s);}).catch(()=>{});return()=>{mounted.current=false;};},[api]);
 useEffect(()=>{writeSetupPreference(storage,{version:1,disposition:'PENDING',agents,step,...(busy||reviewRequired?{needsCheck:step}:{})});title.current?.focus({preventScroll:true});},[agents,step,busy,reviewRequired,storage]);
 useEffect(()=>{if(mode==='TRIAL'&&data.hosted&&(!data.hosted.available||data.hosted.trialAvailable===false||data.hosted.origin))setMode(data.hosted.available?'CODE':'EXISTING');},[data.hosted,mode]);
 // Observable facts advance the screen without asking the owner to prove them.
 useEffect(()=>{if(step!=='PHONE'&&step!=='CLAUDE')return;let live=true,reading=false;const clock=setInterval(()=>setNow(Date.now()),1000);const poll=setInterval(()=>{if(document.hidden||pending.current||reading)return;reading=true;const ticket=readRevision.current;const read=step==='PHONE'&&api.devices?api.devices().then(devices=>({devices})):api.snapshot(agents);void read.then(s=>{if(live&&mounted.current&&ticket===readRevision.current)accept(s);}).catch(()=>{}).finally(()=>{reading=false;});},step==='PHONE'?3000:4000);return()=>{live=false;clearInterval(clock);clearInterval(poll);};},[step,api]);
 useEffect(()=>{if(step!=='CLAUDE'||!api.mcpStatus)return;let live=true;const read=()=>{if(document.hidden)return;void api.mcpStatus!().then(status=>{if(live&&status.endpoint===latest.current.mcpUrl)setMcpPhase(status.phase);}).catch(()=>{});};read();if(mcpAuthorized||!mcpStarted)return()=>{live=false;};const requests=()=>{void api.authorizations?.().then(rows=>{if(live){setAuthRequests(rows);setAuthRequestId(old=>rows.some(row=>row.requestId===old)?old:rows.length===1?rows[0].requestId:'');}}).catch(()=>{});};requests();const timer=setInterval(()=>{read();requests();},2000);return()=>{live=false;clearInterval(timer);};},[step,api,mcpAuthorized,mcpStarted,data.mcpUrl]);
 // The authorization page opens itself once for the request this screen started; "allow" stays with the owner.
 useEffect(()=>{if(step!=='CLAUDE'||!autoOpen.current||!api.openAuthorization||authRequests.length!==1||openedAuthId)return;autoOpen.current=false;const id=authRequests[0].requestId;setOpenedAuthId(id);void api.openAuthorization(id).catch(()=>{setOpenedAuthId('');setError(t('授权页没有自动打开，请点击底部按钮打开。'));});},[step,authRequests,openedAuthId,api]);
 useEffect(()=>{if(step!=='PHONE'){pairIssued.current=false;return;}if(!remote||readyPhone||pair||pairIssued.current||pending.current)return;pairIssued.current=true;void issuePair().catch(()=>{});},[step,remote,readyPhone,pair]);
 useEffect(()=>{if(step!=='WATCH')return;let live=true;setCandidates(null);void Promise.all([api.candidates?.(agents)??Promise.resolve([]),api.watches?.()??Promise.resolve([])]).then(([rows,watches])=>{if(!live)return;const enabled=watches.filter(w=>w.enabled).map(w=>w.threadId);setCandidates(rows);setWatching(enabled);setSelection(old=>{const pick={...old};for(const agent of agents){const group=rows.filter(row=>groupOf(row.id)===agent);if(pick[agent]&&group.some(r=>r.id===pick[agent]))continue;pick[agent]=group.find(r=>enabled.includes(r.id))?.id??(group.length===1?group[0].id:undefined);}return pick;});}).catch(()=>{if(live){setCandidates([]);setError(t('暂时读不到对话，请确认对应应用已打开，再刷新列表。'));}});return()=>{live=false;};},[step,api,listRevision]);
 const groupOf=(id:string):SetupAgent|null=>id.startsWith('claude_code:')?'CLAUDE_CODE':id.includes(':')?null:'CODEX';
 async function copy(text:string,key:string){try{await (api.copyText?api.copyText(text):navigator.clipboard.writeText(text));setCopyFailed('');return true;}catch{setCopyFailed(key);setError(t('复制未成功，请选中下面的内容手动复制。'));return false;}}
 async function issuePair(){const value=await api.pairCode();if(!/^\d{6}$/.test(value.code)||value.expiresAt<=Date.now()/1000)throw Error('SETUP_NOT_READY');if(mounted.current){setPair(value);setNow(Date.now());}}
 async function act(){if(pending.current)return;++readRevision.current;pending.current=true;setBusy(true);setError('');try{
  if(step==='CHOOSE'){const snap=await refresh();setStep(nextSetupStep(plan,'CHOOSE',snap));}
  else if(step==='CODEX'){
   const snap=await refresh();if(!snap.shared)throw Error('SETUP_READ_UNAVAILABLE');if(snap.shared.state==='UNSUPPORTED')throw Error('SETUP_PLATFORM_UNSUPPORTED');
   if(codexReady(snap)){next(snap);return;}
   if(reviewRequired){
    if(codexAction==='PREPARE'&&snap.shared.ready){setReviewRequired(false);setError(t('共享后台已就绪，点击打开共享 Codex。'));}
    else if(!snap.shared.enabled&&snap.shared.state==='DISABLED'){setReviewRequired(false);setError(t('共享尚未开启，下一次点击将重新准备。'));}
    else setError(t(codexAction==='OPEN'?'还不能确认共享窗口状态，请稍后再检查；不会重复打开窗口。':'还不能确认共享状态，请稍后检查；不会重复启动进程。'));
    return;
   }
   let value=snap.shared;
   if(!value.ready){setCodexAction('PREPARE');value=await api.prepareCodex();accept({shared:value});if(!value.ready){setReviewRequired(true);throw Error('SETUP_NOT_READY');}}
   if(!value.desktopConnected){setCodexAction('OPEN');setCloseRequired(false);value=await api.openCodex();accept({shared:value});if(!value.ready||!value.desktopConnected){setReviewRequired(true);throw Error('SETUP_NOT_READY');}}
   next({...snap,shared:value});
  }else if(step==='PHONE'){
   let snap=await refresh();if(phoneReady(snap)){next(snap);return;}
   if(!phoneUrl(snap.origin)||!remoteReady(snap)){
    if(!snap.hosted&&!phoneUrl(snap.origin)){setError(t('连接服务状态暂时不可用，请稍后检查。'));return;}
    if(reviewRequired){if(snap.origin)await api.retryConnection();snap=await refresh();if(!phoneUrl(snap.origin)||!remoteReady(snap)){setError(t('连接结果还不能确认，请稍后检查；不会重复开通试用。'));return;}setReviewRequired(false);}
    else{if(phoneUrl(snap.origin)&&!hostedRenewalRequired(snap)){await api.retryConnection();}else if(mode==='TRIAL'&&snap.hosted?.available&&snap.hosted.trialAvailable!==false&&!snap.hosted.origin){await api.trial();}else if(mode==='CODE'){await api.redeem(activation.trim());setActivation('');}else if(mode==='EXISTING'){const url=phoneUrl(origin);if(!url)throw Error('SETUP_HTTPS_REQUIRED');await api.configure(new URL(url).origin);}else throw Error('SETUP_REMOTE_CHOICE');snap=await refresh();if(!phoneUrl(snap.origin)||!remoteReady(snap)){setReviewRequired(true);setError(t('地址已保存，正在等待连接可用。点击检查连接结果即可。'));return;}}
    if((snap.devices?.length??0)>0){next(snap);return;}
   }
   if(!pair||pairExpired){pairIssued.current=true;await issuePair();}
   else setError(t('还没有收到手机登录。请在手机登录页输入配对码，收到后这里会自动更新。'));
  }else if(step==='CLAUDE'){
   // OAuth may already be complete when this goal is reopened. It does not
   // prove that the independent notification plugin has been installed.
   if(mcpAuthorized&&!pluginInstalled&&!latest.current.claudeLoaded){const result=await api.installClaude();if(!['CONNECTED','WAITING_SESSION'].includes(result.phase))throw Error('SETUP_NOT_READY');setPluginInstalled(true);if(result.phase==='CONNECTED')accept({claudeLoaded:true});}
   if(!mcpAuthorized){
    const restart=!mcpStarted||mcpPhase==='AUTHORIZATION_UNCONFIRMED';
    if(restart&&!manualMcp){
     let snap=await refresh();if(!snap.mcpUrl)throw Error('CLAUDE_MCP_ADDRESS_MISSING');
     if(!snap.claudeLoaded&&!pluginInstalled){const result=await api.installClaude();if(!['CONNECTED','WAITING_SESSION'].includes(result.phase))throw Error('SETUP_NOT_READY');setPluginInstalled(true);if(result.phase==='CONNECTED')accept({claudeLoaded:true});}
     if(!snap.hasGrant){await api.grant();snap=await refresh();if(!snap.hasGrant)throw Error('SETUP_NOT_READY');}
     if(api.mcpStatus){const status=await api.mcpStatus();if(status.endpoint===snap.mcpUrl&&status.phase==='AUTHORIZED'){setMcpPhase('AUTHORIZED');return;}}
     if(api.setupMcp){const result=await api.setupMcp();if(result.endpoint!==snap.mcpUrl)throw Error('CLAUDE_MCP_ENDPOINT_CHANGED');setMcpPhase(result.phase);setOpenedAuthId('');autoOpen.current=true;return;}
     setManualMcp(true);await copy(claudeManualConnectionInstructions(snap.mcpUrl??null,getLanguage()),'MCP');return;
    }
    if(api.mcpStatus){const status=await api.mcpStatus();if(status.endpoint===data.mcpUrl){setMcpPhase(status.phase);if(status.phase==='AUTHORIZED')return;}}
    if(manualMcp||mcpPhase==='MANUAL_AUTH_REQUIRED'){setError(t('还没有确认登录完成。完成后勾选下面的确认项，或等待这里自动更新。'));return;}
    if(api.openAuthorization&&authRequests.length>1&&!authRequestId){setError(t('请先选择刚才发起的连接，再打开授权页。'));return;}
    if(api.openAuthorization&&authRequestId&&openedAuthId!==authRequestId){setOpenedAuthId(authRequestId);await api.openAuthorization(authRequestId);return;}
    setError(t(openedAuthId?'授权页已打开，请切到浏览器点击“允许并连接”。这里会自动检查结果，不必再次打开。':'正在准备授权页，这里会自动刷新。'));return;
   }
   if(!verifyCopied){await copy(t(READ_CHECK),'VERIFY');setVerifyCopied(true);return;}
   const snap=await refresh();if(!snap.claudeLoaded){setError(t('还没有收到插件上线。请确认你新建或重新打开了 Code 对话；原对话正忙时看下方“原对话不能重开”。'));return;}
   setOwnerVerified(true);next(snap);
  }else if(step==='WATCH'){
   if(!api.candidates||!api.watch||!api.watches)throw Error('SETUP_READ_UNAVAILABLE');
   const missing=agents.filter(agent=>!selection[agent]);if(missing.length){setError(t('请为 {0} 选择一条对话。',missing.map(a=>a==='CODEX'?'Codex':'Claude Code').join('、')));return;}
   const rows=await api.watches();const names:string[]=[];
   for(const agent of agents){const id=selection[agent]!;if(!rows.some(row=>row.threadId===id&&row.enabled)){if(reviewRequired)throw Error('SETUP_WATCH_UNCONFIRMED');await api.watch(id);}names.push(candidates?.find(row=>row.id===id)?.label??id);}
   setWatchedNames(names);next();
  }else if(step==='VERIFY'){setPhoneVerified(true);next();}
  else if(step==='DONE'){writeSetupPreference(storage,{version:1,disposition:'COMPLETED',agents,step});onExit('COMPLETED',agents);}
 }catch(reason){const code=String(reason);if(code.includes('CLAUDE_MCP_DIFFERENT_SERVER_PRESERVED')){setError(t('已有名为 agbrio 的其他连接，已保留而未覆盖。请在原 Code 对话的 /mcp 中确认要使用的连接，再回来继续。'));}else if(code.includes('CLAUDE_MCP_ADDRESS_MISSING')){setError(t('还没有可用的连接地址。请返回上一步完成手机连接，地址会自动带过来。'));}else if(code.includes('HOSTED_TRIAL_ALREADY_USED')){setMode('CODE');setError(t('这台设备已使用过试用，请填入激活码，或使用自己的连接地址。'));}else if(step==='PHONE'&&['CODE_ALREADY_USED','CODE_EXPIRED','CODE_UNAVAILABLE','CODE_INVALID','CAPACITY','DEVICE_UNAVAILABLE','TRIAL_EXPIRED'].some(value=>code.includes(value))){setReviewRequired(false);setError(t(code.includes('CODE_ALREADY_USED')?'激活码已在另一台电脑使用。':code.includes('CODE_EXPIRED')?'激活码已到期，请使用新的激活码。':code.includes('CODE_')?'激活码已失效。':code.includes('CAPACITY')?'试用名额暂时已满，请稍后重试或联系作者。':code.includes('TRIAL_EXPIRED')?'试用已结束，请联系作者获取激活码。':'无法识别这台电脑，请使用激活码或联系作者。'));}else if(code.includes('UNCONFIRMED')||code.includes('UNCERTAIN')||code.includes('ACK_UNKNOWN')||step==='PHONE'&&!remote&&!code.includes('SETUP_')){setReviewRequired(true);setError(t('操作结果尚未确认，请先检查当前状态，不会自动重复执行。'));}else if(code.includes('SHARED_DESKTOP_CLOSE_REQUIRED')||code.includes('SHARED_ROUTER_BUSY')||code.includes('SHARED_DESKTOP_BUSY')){setCloseRequired(true);setError('');}else if(code.includes('UAC_CANCELLED'))setError(t('已取消 Windows 授权，本步骤尚未完成。需要时再点一次，系统会重新请求确认。'));else if(code.includes('CLAUDE_'))setError(connectionError(code));else if(code.includes('SETUP_HTTPS_REQUIRED'))setError(t('请填写已部署到这台电脑的 HTTPS 连接地址。'));else if(code.includes('SETUP_PLATFORM_UNSUPPORTED'))setError(t('当前平台尚不支持此项，可跳过引导后使用其他功能。'));else if(code.includes('SETUP_REMOTE_CHOICE'))setError(t('请选择激活码或已有连接地址。'));else setError(t('这一步尚未完成。请检查客户端或网络后重试，也可稍后从设置继续。'));}finally{pending.current=false;if(mounted.current)setBusy(false);}}

 const canTrial=!!data.hosted?.available&&data.hosted.trialAvailable!==false&&!data.hosted.origin;
 const needsRemote=step==='PHONE'&&!remote;
 const codexLabel=reviewRequired?'检查当前状态':closeRequired?'已退出，打开共享 Codex':data.shared?.ready?'打开共享 Codex':'准备并打开共享 Codex';
 const phoneLabel=readyPhone&&remote?'继续':needsRemote?(!data.hosted&&!mobile?'检查连接服务':reviewRequired?'检查连接结果':mobile&&!hostedRenewalRequired(data)?'检查手机连接地址':mode==='TRIAL'&&canTrial?'开启 7 天免费试用':mode==='CODE'?'使用激活码连接':'使用这个地址'):!pair||pairExpired?'生成配对码':'检查手机是否已连接';
 const claudeLabel=!mcpAuthorized?(!mcpStarted||mcpPhase==='AUTHORIZATION_UNCONFIRMED'?'连接 Claude Code':manualMcp||mcpPhase==='MANUAL_AUTH_REQUIRED'?'检查登录结果':authRequests.length>1&&!authRequestId?'选择本次连接后继续':authRequestId&&openedAuthId!==authRequestId?'打开授权页并自动登录':openedAuthId?'检查授权结果':'检查授权准备进度'):!verifyCopied?'复制验证指令':data.claudeLoaded?'Code 已返回本机状态，继续':'检查插件是否上线';
 const action=step==='CHOOSE'?'开始设置':step==='CODEX'?codexLabel:step==='PHONE'?phoneLabel:step==='CLAUDE'?claudeLabel:step==='WATCH'?'开始监听所选对话':step==='VERIFY'?'手机上都能看到并收到回复':'进入 Agbrio';
 const disabled=busy||step==='CHOOSE'&&!agents.length||needsRemote&&!reviewRequired&&(!mobile||hostedRenewalRequired(data))&&(mode==='CODE'&&!activation.trim()||mode==='EXISTING'&&!origin.trim());
 const position=plan.indexOf(step),total=plan.length-2;
 const claudeRows:Row[]=[
  {label:'通知插件',state:data.claudeLoaded?'DONE':pluginInstalled?'ACTIVE':'TODO',note:data.claudeLoaded?'已在 Code 对话中上线':pluginInstalled?'已安装，等 Code 对话加载':undefined},
  {label:'30 天访问授权',state:data.hasGrant?'DONE':'TODO',note:data.hasGrant?'可随时在设置中撤销':undefined},
  {label:'本机 MCP 配置与登录',state:mcpAuthorized?'DONE':mcpStarted?'ACTIVE':'TODO',note:mcpAuthorized?(manualAuthorized?'你已确认登录完成':'官方登录已完成'):mcpStarted?(manualMcp||mcpPhase==='MANUAL_AUTH_REQUIRED'?'需要在 Code 中手动登录':openedAuthId?'等你在浏览器点击“允许”':'正在准备授权页'):undefined},
  {label:'Code 对话读取验证',state:ownerVerified?'DONE':mcpAuthorized?'ACTIVE':'TODO',note:mcpAuthorized&&!ownerVerified?'在 Code 中发送验证指令':undefined},
 ];
 const codexRows:Row[]=[{label:'共享后台',state:data.shared?.ready?'DONE':busy&&codexAction==='PREPARE'?'ACTIVE':'TODO'},{label:'共享 Codex 窗口',state:codexReady(data)?'DONE':busy&&codexAction==='OPEN'?'ACTIVE':'TODO'}];
 const group=(agent:SetupAgent)=>(candidates??[]).filter(row=>groupOf(row.id)===agent);
 const description=preview?.description?.(step)??(step==='PHONE'&&remote?'用手机相机扫描二维码打开 Agbrio，在登录页输入配对码。收到后这里会自动更新。':step==='DONE'&&!agents.includes('CLAUDE_CODE')?'你可以在手机查看进度、回复对话。需要调整时，从设置重新打开引导。':detail[step]);

 return <Dialog.Root open onOpenChange={open=>{if(!open&&!pending.current){writeSetupPreference(storage,{version:1,disposition:'SKIPPED',agents,step});onExit('SKIPPED',agents);}}}><Dialog.Portal><Dialog.Overlay className="first-run-overlay"/><Dialog.Content className="first-run-shell" aria-describedby="first-run-description" onEscapeKeyDown={e=>{if(pending.current)e.preventDefault();}} onPointerDownOutside={e=>e.preventDefault()}>
  <header className="first-run-top"><strong>Agbrio{preview&&<small className="first-run-preview-badge">{t('交互预览')}</small>}</strong>{step!=='CHOOSE'&&step!=='DONE'&&<span className="first-run-progress" aria-label={t('第 {0} 步，共 {1} 步',position,total)}><span className="first-run-dots" aria-hidden>{plan.slice(1,-1).map((item,i)=><i key={item} data-state={i+1<position?'DONE':i+1===position?'ACTIVE':'TODO'}/>)}</span>{position} / {total}</span>}<Dialog.Close className="first-run-skip" disabled={busy}>{t('跳过引导')}</Dialog.Close></header>
  <main className="first-run-content"><div className="first-run-body"><Dialog.Title asChild><h1 ref={title} tabIndex={-1}>{t(headings[step])}</h1></Dialog.Title><Dialog.Description id="first-run-description">{t(description)}</Dialog.Description>
   {step==='CHOOSE'&&<div className="first-run-agent-options">{(['CODEX','CLAUDE_CODE'] as SetupAgent[]).map(agent=><label key={agent}><input type="checkbox" checked={agents.includes(agent)} onChange={()=>setAgents(old=>old.includes(agent)?old.filter(a=>a!==agent):[...old,agent])}/>{agent==='CODEX'?<Terminal size={24}/>:<MessagesSquare size={24}/>}<span><strong>{agent==='CODEX'?'Codex':'Claude Code'}</strong><small>{t(agent==='CODEX'?'共享电脑上的 Codex 对话':'安装插件并连接当前 Code 对话')}</small></span><Check size={19} className="first-run-selected"/></label>)}</div>}

   {step==='CODEX'&&<><Checklist rows={codexRows}/>
    {closeRequired&&<div className="first-run-callout" role="status"><strong>{t('请先退出普通 Codex')}</strong><p>{t('在已打开的 Codex 窗口保存工作、等任务结束，然后从任务栏或菜单退出它。退出后回到这里点底部按钮，Agbrio 会打开共享模式的 Codex。不会停止或改动你的任务。')}</p></div>}
    {reviewRequired&&codexAction==='OPEN'&&<button type="button" className="first-run-text" disabled={busy} onClick={()=>{setReviewRequired(false);setError(t('已允许重新尝试。若共享窗口其实已打开，请先关掉多余的窗口。'));}}>{t('确认窗口没有打开？重新尝试')}</button>}</>}

   {needsRemote&&!reviewRequired&&!!data.hosted&&(!mobile||hostedRenewalRequired(data))&&<div className="first-run-remote"><fieldset><legend>{t('选择连接方式')}</legend>{canTrial&&<label><input type="radio" name="remote-mode" checked={mode==='TRIAL'} onChange={()=>setMode('TRIAL')}/>{t('免费试用 7 天')}</label>}<label><input type="radio" name="remote-mode" checked={mode==='CODE'} onChange={()=>setMode('CODE')}/>{t('已有激活码')}</label>{!mobile&&<label><input type="radio" name="remote-mode" checked={mode==='EXISTING'} onChange={()=>setMode('EXISTING')}/>{t('已有自己的连接地址')}</label>}</fieldset>{mode==='CODE'&&<label>{t('激活码')}<input type="password" autoComplete="off" value={activation} onChange={e=>setActivation(e.target.value)}/></label>}{mode==='EXISTING'&&<label>{t('已部署的 HTTPS 地址')}<input type="url" inputMode="url" value={origin} onChange={e=>setOrigin(e.target.value)} placeholder="https://"/></label>}{!canTrial&&mode==='TRIAL'&&<p>{t('试用当前不可用，请选择上面的其他连接方式。')}</p>}<p className="first-run-hint">{t('地址准备好后会直接显示手机二维码。')}</p></div>}
   {step==='PHONE'&&remote&&!readyPhone&&<div className="first-run-phone">{mobile&&<PhoneQr url={mobile}/>}{preview&&<small>{t('演示二维码，请勿用真实手机扫描')}</small>}<output aria-label={t('一次性配对码')}>{pairExpired?t('已过期'):pair?.code??'——'}</output>{pair&&!pairExpired&&<small>{t('{0} 秒内有效 · 使用一次',Math.max(0,Math.ceil(pair.expiresAt-now/1000)))}</small>}{mobile&&<span className="first-run-phone-url">{mobile}</span>}<p className="first-run-waiting" role="status"><LoaderCircle size={15} className="v4-progress-spinner" aria-hidden/>{t('等待手机登录…')}</p></div>}
   {step==='PHONE'&&readyPhone&&remote&&<><p className="first-run-success" role="status"><Check size={18} aria-hidden/>{t('已收到手机登录')}{data.devices?.[0]?.label?` · ${data.devices[0].label}`:''}</p>
    <details className="first-run-help"><summary>{t('可选：添加到手机桌面并开启通知')}</summary><PwaInstallGuide expanded/><p>{t('从新图标打开后又要求登录？在这里生成新码，填到手机当前登录页；用过的码不能再用。')}</p><button type="button" className="first-run-text" disabled={busy} onClick={async()=>{if(pending.current)return;pending.current=true;setBusy(true);try{const value=await api.pairCode();if(!/^\d{6}$/.test(value.code)||value.expiresAt<=Date.now()/1000)throw Error('SETUP_NOT_READY');setHomeCode(value);setNow(Date.now());setError('');}catch{setError(t('暂时无法生成配对码，请检查电脑连接后重试。'));}finally{pending.current=false;setBusy(false);}}}>{t('生成新的手机登录码')}</button>{homeCode&&<output className="first-run-inline-code">{homeCode.expiresAt>now/1000?homeCode.code:t('已过期')}</output>}</details></>}

   {step==='CLAUDE'&&<><Checklist rows={claudeRows}/>
    {!mcpAuthorized&&!manualMcp&&mcpStarted&&api.openAuthorization&&authRequests.length>1&&<div className="first-run-chat-list">{authRequests.map(row=><label key={row.requestId}><input type="radio" name="owner-auth-request" checked={authRequestId===row.requestId} onChange={()=>setAuthRequestId(row.requestId)}/><span><strong>{row.clientName}</strong><small>{row.redirectUri}</small></span></label>)}<p>{t('选中刚才发起的连接。授权页会自动登录；“允许并连接”仍由你本人点击。')}</p></div>}
    {!mcpAuthorized&&openedAuthId&&!manualMcp&&mcpPhase!=='MANUAL_AUTH_REQUIRED'&&<div className="first-run-callout" role="status"><strong>{t('切到电脑浏览器')}</strong><p>{t('Agbrio 授权页已打开并自动登录。核对申请方后点击“允许并连接”，完成后这里会自动打勾，不用回来点确认。')}</p><button type="button" className="first-run-text" disabled={busy} onClick={()=>{setOpenedAuthId('');}}>{t('授权页没打开或已过期？重新打开')}</button></div>}
    {!mcpAuthorized&&(manualMcp||mcpPhase==='MANUAL_AUTH_REQUIRED')&&<div className="first-run-callout"><strong>{t(manualMcp?'在 Code 对话中添加并登录':'在 Code 对话中登录')}</strong>{manualMcp?<p>{t('接入指令已复制。打开 Claude 桌面应用（不是网页）→ Code 页签，在要用的对话里按 Ctrl+V 发送；它会添加这台电脑的地址并提示登录。')}</p>:<p>{t('连接配置已完成。请在原 Code 对话输入 /mcp，选择 agbrio 并登录，无需重新添加。')}</p>}
     {copyFailed==='MCP'&&<textarea readOnly aria-label={t('手动复制接入指令')} value={claudeManualConnectionInstructions(data.mcpUrl??null,getLanguage())} onFocus={e=>e.currentTarget.select()}/>}
     <details className="first-run-help"><summary>{t('授权页要求配对码？')}</summary><button type="button" className="first-run-text" disabled={busy} onClick={async()=>{try{const value=await api.pairCode();if(!/^\d{6}$/.test(value.code))throw Error('SETUP_NOT_READY');setAuthPair(value);setNow(Date.now());await copy(value.code,'AUTH');}catch{setError(t('暂时无法生成登录码，请稍后重试。'));}}}>{t('生成并复制授权页登录码')}</button>{authPair&&<><output className="first-run-inline-code">{authPair.expiresAt>now/1000?authPair.code:t('已过期')}</output><small>{t('只填在浏览器的 Agbrio 授权页，不要发到 AI 对话。')}</small></>}</details>
     <label className="first-run-confirm"><input type="checkbox" checked={manualAuthorized} onChange={e=>setManualAuthorized(e.target.checked)}/><span>{t('我已在授权页点击允许，Code 显示 agbrio 已连接')}</span></label></div>}
    {mcpAuthorized&&<div className="first-run-callout"><strong>{t('到 Claude 桌面应用验证')}</strong><ol className="first-run-instructions"><li>{t('打开电脑上的 Claude 桌面应用（不是浏览器网页），进入 Code 页签。')}</li><li>{t('新建一条对话，或等任务结束后重新打开要管理任务的原对话，让 Code 加载刚配置的插件和 MCP；接下来还要检查是否实际可用。')}</li><li>{t(verifyCopied?'在输入框按 Ctrl+V 粘贴验证指令并发送，等它回复这台电脑的 Agbrio 状态。':'点底部按钮复制验证指令，再到这条对话粘贴发送。')}</li></ol>
     {verifyCopied&&<p className="first-run-hint">{t('看到它报告本机状态和授权范围才算成功；只说“已添加”或“已授权”还不算。插件上线会自动显示在上方。')}</p>}
     {copyFailed==='VERIFY'&&<textarea readOnly aria-label={t('手动复制读取验证指令')} value={t(READ_CHECK)} onFocus={e=>e.currentTarget.select()}/>}
     <details className="first-run-help"><summary>{t('原对话正忙不能重开，或 Code 看不到 Agbrio 工具？')}</summary><p>{t('等任务结束后在原对话输入下面的命令重载插件；输入 /mcp 可检查 agbrio 是否已连接。重载插件不会刷新 MCP 工具列表；仍没有工具时，重新打开原对话或在同一项目新开 Code 对话，再发送验证指令。')}</p><code className="first-run-command">/reload-plugins</code><button type="button" className="first-run-text" onClick={()=>void copy('/reload-plugins','RELOAD')}><Copy size={14} aria-hidden/>{t('复制命令')}</button><p>{t('Code 提示尚未授权：回到浏览器的 Agbrio 授权页点击允许并连接，再在同一条对话重试。')}</p></details></div>}
    {!mcpAuthorized&&!mcpStarted&&data.mcpUrl&&<details className="first-run-help"><summary>{t('自动配置不可用？查看地址和手动方式')}</summary><label className="first-run-endpoint">{t(preview?'演示地址，不要添加到真实账号':'这台电脑的 MCP 地址')}<input readOnly value={data.mcpUrl} onFocus={e=>e.currentTarget.select()}/></label><p>{t('把这个地址加到本机 Claude Code 的用户级 MCP（不是 Claude 网页），或在 Code 对话中发送接入指令让它检查配置。保留现有连接。')}</p><button type="button" className="first-run-text" onClick={()=>void copy(claudeManualConnectionInstructions(data.mcpUrl!,getLanguage()),'MCP')}>{t('复制接入指令')}</button></details>}</>}

   {step==='WATCH'&&<div className="first-run-watch">{candidates===null?<p role="status">{t('正在读取对话…')}</p>:agents.map(agent=>{const rows=group(agent);return <fieldset key={agent} className="first-run-chat-list"><legend>{agent==='CODEX'?'Codex':'Claude Code'}</legend>{rows.length?rows.map(row=><label key={row.id}><input type="radio" name={`setup-chat-${agent}`} checked={selection[agent]===row.id} onChange={()=>setSelection(old=>({...old,[agent]:row.id}))}/><span><strong>{row.label}</strong><small>{[row.projectLabel??row.projectProvenance??'',watching.includes(row.id)?t('已在监听'):''].filter(Boolean).join(' · ')}</small></span></label>):<p>{t(agent==='CODEX'?'还没有读到 Codex 对话。在共享 Codex 中打开或新建一条对话后刷新列表。':'还没有读到 Code 对话。在 Claude 桌面应用的 Code 页签打开刚才验证的对话后刷新列表。')}</p>}</fieldset>;})}
    <button type="button" className="first-run-text" disabled={busy} onClick={()=>setListRevision(v=>v+1)}>{t('刷新列表')}</button></div>}

   {step==='VERIFY'&&<><ul className="first-run-summary">{watchedNames.map(name=><li key={name}><MessagesSquare size={18}/>{name}</li>)}</ul><button type="button" className="first-run-text" disabled={busy} onClick={()=>next()}>{t('稍后再核对')}</button></>}

   {step==='DONE'&&<><ul className="first-run-summary">
    {agents.includes('CODEX')&&<li data-state={codexReady(data)?'DONE':'TODO'}>{codexReady(data)?<Check size={18}/>:<Circle size={16}/>}{t(codexReady(data)?'Codex 共享已连接（系统已观测）':'Codex 共享待检查')}</li>}
    {agents.includes('CLAUDE_CODE')&&<><li data-state={data.claudeLoaded?'DONE':'TODO'}>{data.claudeLoaded?<Check size={18}/>:<Circle size={16}/>}{t(data.claudeLoaded?'Claude 插件已上线（系统已观测）':'Claude 插件待检查')}</li><li data-state={ownerVerified?'DONE':'TODO'}>{ownerVerified?<Check size={18}/>:<Circle size={16}/>}{t(ownerVerified?preview?'演示 MCP 读取成功':'MCP 读取：你已确认':'MCP 读取：待本人核对')}</li></>}
    <li data-state={readyPhone?'DONE':'TODO'}>{readyPhone?<Check size={18}/>:<Smartphone size={18}/>}{t(readyPhone?'手机已配对（系统已观测）':'手机配对待检查')}</li>
    <li data-state={phoneVerified?'DONE':'TODO'}>{phoneVerified?<Check size={18}/>:<Circle size={16}/>}{t(phoneVerified?preview?'演示手机收发已验证':'手机跟进：你已确认':'手机跟进：待核对')}</li>
    {watchedNames.map(name=><li key={name} data-state="DONE"><Check size={18}/>{t('正在监听')} · {name}</li>)}</ul>
    {agents.includes('CLAUDE_CODE')&&<div className="first-run-callout"><strong>{t('让 Claude 开始管理任务（可选）')}</strong><p>{t('在刚才验证过的 Code 对话里直接交代任务。它能读取进度、按你的要求创建或跟进 Codex 执行对话，需要你决定时会通知你。')}</p><button type="button" className="first-run-text" onClick={async()=>{if(await copy(t(STARTER),'STARTER'))setStarterCopied(true);}}><Copy size={14} aria-hidden/>{t(starterCopied?'已复制起步指令':'复制只读起步指令')}</button>{copyFailed==='STARTER'&&<textarea readOnly aria-label={t('手动复制起步指令')} value={t(STARTER)}/>}</div>}</>}
   {companion?.(step)}
   {error&&<p className="first-run-error" role="alert">{error}</p>}
  </div></main><footer className="first-run-footer"><div>{step!=='CHOOSE'&&!busy&&<button type="button" className="first-run-back" onClick={()=>go(plan[Math.max(0,position-1)])}><ArrowLeft size={17}/>{t('上一步')}</button>}</div><button type="button" className="first-run-primary" disabled={disabled} onClick={()=>void act()}>{busy?<><LoaderCircle size={18} className="v4-progress-spinner"/>{t('正在处理…')}</>:<>{t(action)}<ArrowRight size={18}/></>}</button></footer>
 </Dialog.Content></Dialog.Portal></Dialog.Root>;
}
