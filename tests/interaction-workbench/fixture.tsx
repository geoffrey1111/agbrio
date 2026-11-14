import {markSessionLive} from '../../src/mobile/startup';
import {AssistantSettings,type AssistantSettingsApi} from '../../src/features/workbench/AssistantSettings';
import {PiSettings,piSettingsApi} from '../../src/features/workbench/PiSettings';
import {LocalSafetySettings,localSafetyApi} from '../../src/features/workbench/LocalSafetySettings';
import {DesktopWebAccess,type WebAccessApi} from '../../src/features/workbench/DesktopWebAccess';
import {DesktopUpdateSettings,type UpdateApi,type UpdateView} from '../../src/features/workbench/DesktopUpdateSettings';
import {NotificationDeliveryPanel,type DeliveryApi,type DeliveryView} from '../../src/features/workbench/NotificationDeliveryPanel';
import {CodexConnection,type CodexConnectionStatus} from '../../src/features/workbench/CodexConnection';
import {demoManagement,demoConversation} from './demo-management';
import {useState} from 'react';
import {createRoot} from 'react-dom/client';
import * as Dialog from '@radix-ui/react-dialog';
import {UnifiedWorkbench,type WorkbenchSurface} from '../../src/features/workbench/UnifiedWorkbench';
import type {OverviewApi} from '../../src/features/workbench/SettingsHome';
import {RelayReviewSurface} from '../../src/features/workbench/RelayReviewSurface';
import {NotificationAssistantSettings} from '../../src/features/workbench/CodexNotifications';
import type {CodexWatch, NotificationApi} from '../../src/features/workbench/CodexNotifications';
import type {WatchChatApi} from '../../src/features/workbench/watchChatApi';
import type {RoleReview} from '../../src/features/workbench/roleReviewDrafts';
import '../../src/styles/unified-workbench-v3.css';
import '../../src/styles/warm-workbench.css';
import '../../src/styles/web-login.css';
import '../../src/styles/watch-chat.css';
import '../../src/styles/bridge-v5.css';
import '../../src/styles/native-router.css';
import '../../src/styles/interaction-motion.css';
import '../../src/styles/conversation-v6.css';
import '../../src/styles/agbrio-v6-theme.css';
import '../../src/styles/mobile-inputs.css';
import '../../src/styles/conversation-polish.css';
import '../../src/styles/agbrio-v7.css';
import '../../src/styles/agbrio-v7-r2.css';

// Read-only provider projections; writes are local fixture state or rejected.
const forbidden=async():Promise<never>=>{throw Error('FIXTURE_NO_PROVIDER_WRITES');};
const now=Date.now();
const watches:CodexWatch[]=[
 {threadId:'fixture-question',label:'项目交付检查',cwd:'D:/fixture/project',enabled:true,generation:1,checkedAt:now,lastMessageAt:now,errorCode:null,snapshot:{state:'ACTION_REQUIRED',turnId:'question-turn',itemId:'question-item',text:'哪一种方案应进入下一轮？'}},
 {threadId:'fixture-running',label:'界面回归检查',cwd:'D:/fixture/project',enabled:true,generation:1,checkedAt:now,lastMessageAt:now,errorCode:null,snapshot:{state:'RUNNING',turnId:'running-turn',itemId:'running-item',text:'正在检查手机端对话列表。'}},
];
function asWatch(id:string):CodexWatch|undefined{const row=demoConversation(id);return row?{threadId:id,label:row.label,cwd:row.cwd,enabled:true,generation:row.generation,checkedAt:now,errorCode:null,snapshot:{state:row.state,turnId:null,itemId:null,text:''}}:undefined;}
const extraWatches:string[]=[];
const chat:WatchChatApi={state:async id=>{const watch=watches.find(row=>row.threadId===id)??asWatch(id);if(!watch)throw Error('FIXTURE_UNKNOWN_CONVERSATION');return {watch,provider:'CODEX',inputReady:true,host:'演示电脑',ownedTurnId:null,externalBusy:false,replies:[],requests:[],goal:null};},command:async<T,>(input:Parameters<WatchChatApi['command']>[0])=>input.action==='HISTORY'?{messages:[{id:'sample',turnId:'fixture-turn',role:'assistant',text:'请核对交付清单和手机端入口。'}],nextCursor:null} as T:forbidden()};
const watchApi:NotificationApi={cachedWatches:()=>watches,watches:async()=>[...watches,...extraWatches.map(id=>asWatch(id)!)],threads:async()=>({complete:true,threads:[]}),connect:forbidden,enable:async id=>{if(!asWatch(id))return forbidden();if(!extraWatches.includes(id))extraWatches.push(id);},pause:forbidden,feed:async()=>({events:[],hasMore:false,nextCursor:0}),event:forbidden,webUrl:async()=>null,chat};
const initial:RoleReview={role:'DECISION',reply:{id:'fixture-reply',endpointId:'fixture-planner',text:'请复核手机端和桌面端的交付清单。'},text:'请复核手机端和桌面端的交付清单。',options:[],selected:[]};
const preview=document.documentElement.hasAttribute('data-offline-preview')||new URLSearchParams(location.search).has('preview');
document.documentElement.toggleAttribute('data-offline-preview',preview);
if(preview)markSessionLive(true);
const overviewApi:OverviewApi={shared:async()=>({state:'CONNECTED',ready:true,desktopConnected:true}) as Awaited<ReturnType<OverviewApi['shared']>>,pi:async()=>({installed:true,administratorBackend:{ready:true}}),devices:async()=>[{}],update:async()=>({state:'UP_TO_DATE',currentVersion:'0.1.35-rc.147'}) as Awaited<ReturnType<OverviewApi['update']>>};
const args=new URLSearchParams(location.search);
const phone=args.has('phone');
const pause=async()=>{await new Promise(resolve=>setTimeout(resolve,300));};
const failure=(key:string)=>{if(args.get('fail')===key)throw Error('DEMO_'+key.toUpperCase());};
const grantView:Awaited<ReturnType<AssistantSettingsApi['view']>>={bridges:[],grants:[],mcpUrl:'https://example.invalid/mcp'};
const assistants:AssistantSettingsApi={view:async()=>{await pause();failure('assistant');return structuredClone(grantView);},create:async input=>{await pause();grantView.grants.push({...input,id:crypto.randomUUID(),scope:'INSTANCE',workstreamId:'',sourceRole:'',rules:[],revokedAt:null});},revoke:async id=>{await pause();grantView.grants=grantView.grants.filter(g=>g.id!==id);},openHelp:async()=>{}};
const pi:Awaited<ReturnType<typeof piSettingsApi.read>>={installed:true,version:'0.1.0',administratorBackend:{ready:false},scopes:[],assistantGrants:[{id:'fixture-grant',label:'演示助手'}]};
const readPi=async()=>{await pause();failure('pi');return structuredClone(pi);};
const piApi:typeof piSettingsApi={read:readPi,prepareAdministrator:async()=>{pi.administratorBackend={ready:true};return readPi();},authorize:async(root,writable)=>{pi.scopes.push({id:crypto.randomUUID(),root,writable,enabled:true,generation:1});return readPi();},authorizeWorkspace:async input=>{pi.scopes.push({id:crypto.randomUUID(),root:input.roots[0]??'D:/项目',writable:input.writable,enabled:true,generation:1,policy:input});return readPi();},revoke:async id=>{pi.scopes=pi.scopes.filter(s=>s.id!==id);return readPi();}};
const targets=[{threadId:'art',generation:1,cwd:'D:/项目/铸阵',label:'接续角色建模与ARP制作'},{threadId:'manager-b',generation:1,cwd:'D:/项目/铸阵',label:'工程管理'}];
const safety:Awaited<ReturnType<typeof localSafetyApi.read>>={settings:{recoveryEnabled:false,alertTarget:null},collectorAvailable:true,targets,recent:[{id:'demo',kind:'RESOURCE',status:'DELIVERED',threadId:'art',updatedAt:now}]};
const safetyApi:typeof localSafetyApi={read:async()=>{await pause();failure('protection');return structuredClone(safety);},save:async draft=>{await pause();safety.settings=draft;return structuredClone(safety);}};
const monitor={read:async()=>({config:{SchemaVersion:1,Enabled:false,SampleSeconds:30,AvailableMemoryPercent:10,CommitPercent:95,RepeatMinutes:15,Disks:[{Drive:'D:',Enabled:true,FreePercent:10}]},drives:[{drive:'D:',label:'项目盘',totalBytes:107374182400,freeBytes:21474836480,freePercent:20}],availableBytes:10737418240,physicalBytes:34359738368,running:false,installed:true,legacy:false,startupEnabled:false}),apply:async (config:Awaited<ReturnType<typeof monitor.read>>['config'])=>({...await monitor.read(),config,running:config.Enabled})};
let update:UpdateView={state:'IDLE',currentVersion:'0.1.35-rc.147',version:null,notes:null,releaseUrl:null,downloaded:0,total:null,error:null};
const updater:UpdateApi={status:async()=>update,check:async()=>{await pause();failure('update');return update={...update,state:'AVAILABLE',version:'0.1.36-demo',notes:'演示更新：检查、下载和安装都在这个页面模拟。'};},download:async()=>{await pause();return update={...update,state:'READY'};},install:async(_,confirmed)=>{await pause();if(!confirmed)throw Error('UPDATE_TASK_RUNNING');update={...update,state:'UP_TO_DATE',currentVersion:'0.1.36-demo'};},openRelease:async()=>{}};
let conn={method:'CUSTOM_HTTPS',origin:'https://example.invalid',port:47114,configured:true};
let devices=[{id:'demo-device',label:'演示手机',createdAt:now/1000,expiresAt:now/1000+86400}];
const deviceApi:WebAccessApi={connection:async()=>conn,configure:async({method,origin})=>{await pause();if(!origin.startsWith('https://')||new URL(origin).pathname!=='/')throw Error('MOBILE_ORIGIN');return conn={...conn,method,origin};},url:async()=>conn.origin+'/mobile',devices:async()=>structuredClone(devices),issue:async()=>{await pause();failure('pair');return {code:'123456',expiresAt:Date.now()/1000+60};},revoke:async id=>{await pause();devices=devices.filter(d=>d.id!==id);},availability:async()=>({state:'PUBLIC_READY',localReady:true,tunnelReady:true,publicReady:true,recoveryAllowed:false,checkedAt:now,recoveryCount:0})};
const delivery:DeliveryView={account:'',recipient:'',hasSecret:false,windowsStatus:'ENABLED',channels:[{channel:'WINDOWS',enabled:false},{channel:'WEB',enabled:false},{channel:'EMAIL',enabled:false}],history:[]};
const notifications:DeliveryApi={desktop:!phone,settings:async()=>{await pause();failure('notification');return structuredClone(delivery);},command:async raw=>{await pause();const x=raw as {action:string;channel:string;enabled:boolean;account:string;recipient:string;password:string|null};if(x.action==='CHANNEL')delivery.channels=delivery.channels.map(c=>c.channel===x.channel?{...c,enabled:x.enabled}:c);if(x.action==='EMAIL'){if(x.password&&x.password.length!==16)throw Error('EMAIL_APP_PASSWORD_FORMAT_INVALID');Object.assign(delivery,{account:x.account,recipient:x.recipient,hasSecret:!!x.password||delivery.hasSecret});}if(x.action==='FORGET_PASSWORD'){delivery.hasSecret=false;delivery.channels=delivery.channels.map(c=>c.channel==='EMAIL'?{...c,enabled:false}:c);}if(x.action==='TEST')delivery.history.unshift({eventSequence:delivery.history.length+1,channel:x.channel,status:'SENT',attempts:1,errorCode:null,updatedAt:Date.now()});return 'demo';}};
const connection=async():Promise<CodexConnectionStatus>=>{await pause();failure('codex');return {enabled:true,ready:true,desktopConnected:true,state:'LIGHT_CONNECTED'};};
function Fixture(){
 const [restored,setRestored]=useState(false),[checkedRuntime,setCheckedRuntime]=useState(false);
 const [selected,setSelected]=useState<string>(),[review,setReview]=useState(initial),[sent,setSent]=useState(false);
 const [surface,setSurface]=useState<WorkbenchSurface>(preview?(args.has('management')?'MANAGEMENT':'SETTINGS'):'BRIDGES'),[device,setDevice]=useState(0);
 return <>{preview&&<div className="fixture-banner"><span>Agbrio · 演示数据</span><a href="?preview">桌面设置</a><a href="?preview&phone">手机设置</a><a href="?preview&management&legacy&inactivity">管理区</a><a href="?preview&management&uncertain">回执待核对</a></div>}
 <UnifiedWorkbench surface={surface} onSurfaceChange={setSurface} items={[{id:'fixture-bridge',name:'交付检查 Bridge',lifecycle:'ACTIVE'},{id:'fixture-recycle',name:'演示旧 Bridge',lifecycle:restored?'ACTIVE':'TRASHED'}]} onLifecycleChange={async id=>{if(id==='fixture-recycle')setRestored(true);}} selectedWorkstreamId={selected} onSelectWorkstream={setSelected} runtime={preview?{checks:[{id:'codex',label:'Codex 后端',state:'READY',detail:'演示连接已就绪。',action:()=>setCheckedRuntime(true),actionLabel:'检查状态'},...(phone?[{id:'router-core',label:'电脑',state:'UNAVAILABLE' as const,detail:'演示：手机暂时连不到电脑。'}]:[])],notice:checkedRuntime?'演示连接检查完成。':null}:undefined} watchApi={watchApi} managementApi={demoManagement} onRenameBridge={async()=>{}} onCreateBridge={async()=>{}}
  overviewApi={preview&&!phone?overviewApi:undefined}
  codexConnectionPanel={preview&&!phone?<CodexConnection status={connection}/>:undefined}
  piSettingsPanel={preview&&!phone?<PiSettings api={piApi}/>:undefined}
  runtimeProtectionPanel={preview&&!phone?<LocalSafetySettings api={safetyApi} monitor={monitor}/>:undefined}
  onOpenWebAccess={preview&&!phone?()=>setDevice(v=>v+1):undefined}
  globalActions={preview&&!phone?<DesktopWebAccess api={deviceApi} openRequest={device}/>:undefined}
  updatePanel={preview&&!phone?<DesktopUpdateSettings updateApi={updater}/>:undefined}
  quotaPanel={preview?<div className="fixture-panel fixture-usage"><span>Codex · 每周剩余 <strong>64%</strong></span><span>Claude Code · 每周剩余 <strong>78%</strong></span></div>:undefined}
  notificationSettings={preview?<><NotificationDeliveryPanel api={notifications}/><NotificationAssistantSettings api={watchApi}/></>:<p>演示通知设置</p>}
  assistantPanel={preview?<AssistantSettings api={assistants}/>:<p>演示助手连接</p>}
  bridgePanel={<Dialog.Root><Dialog.Trigger asChild><button type="button">打开转发审阅</button></Dialog.Trigger><Dialog.Portal><Dialog.Overlay className="r2-relay-overlay"/><Dialog.Content className="r2-relay" aria-describedby={undefined} onEscapeKeyDown={event=>{if(event.target instanceof Element&&event.target.closest('.r2-draft-menu'))event.preventDefault();}}><Dialog.Close id="fixture-relay-close" hidden/><RelayReviewSurface review={review} busy={false} oldReview={false} sourceLabel="界面与交互设计管理（桌面工作区）" targetLabel="手机与桌面交付执行团队 / 2026 年 11 月完整交互验收 / 只读演示会话" media={forbidden} error={null} connectionAction={sent?<p role="status">演示确认，未发送真实内容</p>:null} change={setReview} attachmentChange={()=>{}} submit={()=>setSent(true)} close={()=>document.getElementById('fixture-relay-close')?.click()} discard={()=>setReview(initial)} checkStatus={()=>{}}/></Dialog.Content></Dialog.Portal></Dialog.Root>}/>
 </>;
}
createRoot(document.getElementById('root')!).render(<Fixture/>);
