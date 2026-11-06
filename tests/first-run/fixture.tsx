/** Disposable interactive preview: every connection below is simulated; nothing touches a real account, client, phone or task. */
import {createRoot} from 'react-dom/client';
import {useSyncExternalStore,type ReactNode} from 'react';
import {DesktopFirstRun} from '../../src/features/onboarding/DesktopFirstRun';
import {READ_CHECK,type FirstRunPreview} from '../../src/features/onboarding/FirstRunWizard';
import type {FirstRunApi} from '../../src/features/onboarding/firstRunApi';
import type {SetupSnapshot,SetupStep} from '../../src/features/onboarding/firstRunModel';
import {setLanguagePreference} from '../../src/i18n';
import '../../src/styles/agbrio-v6-theme.css';
import '../../src/styles/agbrio-v7.css';
import '../../src/styles/agbrio-v7-r2.css';
import './fixture.css';
const params=new URLSearchParams(location.search);
setLanguagePreference(params.get('lang')==='en'?'en':'zh-CN');
const state:SetupSnapshot={existingUse:false,shared:{enabled:true,ready:true,desktopConnected:false,state:'LIGHT_WAITING'},claudeLoaded:false,hasGrant:false,mcpUrl:null,origin:null,publicReady:false,devices:[],hosted:{available:true,enabled:false,state:'DISABLED',origin:null,expiresAt:null,trialAvailable:true}};
const events:string[]=[],preferences=new Map<string,string>(),watched:{threadId:string;enabled:boolean}[]=[];
let lastCode='',mcpStarted=false,authPageOpen=false,oauthAllowed=false,buffer='',readResult=false,phoneReplied=false,revision=0,issued=0;
const listeners=new Set<()=>void>();function notify(){revision++;listeners.forEach(fn=>fn());}const subscribe=(fn:()=>void)=>{listeners.add(fn);return()=>{listeners.delete(fn);};};
function connect(origin='https://my-computer.fixture.invalid'){state.origin=origin;state.mcpUrl=origin+'/mcp';state.publicReady=true;return state.hosted={available:true,enabled:true,state:'READY',origin,expiresAt:Date.now()+7*86400000,kind:'TRIAL',trialAvailable:false};}
const request={requestId:'f'.repeat(64),clientName:'Claude Code · 我的管理助手',redirectUri:'http://127.0.0.1:4567/callback',resource:'',expiresAt:Date.now()+600000};
const api:FirstRunApi={
 setupMcp:async()=>{mcpStarted=true;events.push('MCP_CONFIGURED');notify();return {phase:'AUTHORIZING',endpoint:state.mcpUrl!};},
 mcpStatus:async()=>({phase:oauthAllowed?'AUTHORIZED':mcpStarted?'AUTHORIZING':'NOT_STARTED',endpoint:state.mcpUrl!}),
 authorizations:async()=>mcpStarted&&!oauthAllowed?[{...request,resource:state.mcpUrl!}]:[],
 openAuthorization:async()=>{authPageOpen=true;events.push('AUTH_PAGE_OPENED');notify();},
 copyText:async text=>{buffer=text;notify();},
 snapshot:async()=>({...state,devices:state.devices?.slice()}),devices:async()=>state.devices?.slice(),
 installClaude:async()=>{events.push('CLAUDE_INSTALL');return {phase:'WAITING_SESSION',writesEnabled:false};},
 trial:async()=>{events.push('TRIAL');return connect();},redeem:async()=>{events.push('REDEEM');return connect();},configure:async origin=>{events.push('CUSTOM_ADDRESS');connect(origin);},retryConnection:async()=>{},
 pairCode:async()=>{events.push('PAIR_CODE');issued++;lastCode=issued===1?'123456':'654321';notify();return {code:lastCode,expiresAt:Date.now()/1000+180};},
 grant:async()=>{events.push('GRANT');state.hasGrant=true;},
 candidates:async()=>[{id:'demo-codex-chat',label:'Codex · 我的第一个任务',projectLabel:'示例项目'},{id:'claude_code:demo-manager',label:'Claude Code · 我的管理助手',projectLabel:'示例项目'}],
 watches:async()=>watched.slice(),watch:async threadId=>{events.push('WATCH:'+threadId);watched.push({threadId,enabled:true});},
};
const preview:FirstRunPreview={description:step=>({
 CODEX:'本页模拟正常打开 Codex。点下方演示窗口里的按钮，不会操作你真实的 Codex。',
 CLAUDE:'本页模拟 Agbrio 调用本机官方 Claude Code 安装插件、创建授权并配置 MCP。演示浏览器和演示 Code 显示在下方，不会改动真实客户端。',
} as Partial<Record<SetupStep,string>>)[step]};
function Demo({title,children}:{title:string;children:ReactNode}){return <section className="demo-companion" aria-label={title}><strong>{title}</strong>{children}</section>;}
function Companion({step}:{step:SetupStep}){
 useSyncExternalStore(subscribe,()=>revision);
 if(step==='CHOOSE')return <p className="demo-notice">完整交互预览：外部应用（手机、浏览器、Claude Code）会以“演示窗口”显示在本页，不需要真的切换窗口。</p>;
 if(step==='CODEX'&&!state.shared?.desktopConnected)return <Demo title="演示桌面"><button type="button" onClick={()=>{state.shared={enabled:true,ready:true,desktopConnected:true,state:'LIGHT_CONNECTED'};events.push('CODEX_OPENED');notify();}}>打开演示 Codex</button></Demo>;
 if(step==='PHONE'&&lastCode&&!state.devices?.length)return <Demo title="演示手机 · Agbrio 登录页"><label>配对码<input readOnly value={lastCode}/></label><button type="button" onClick={()=>{state.devices=[{id:'demo-phone',label:'iPhone · Demo'}];events.push('PHONE_PAIRED');notify();}}>在演示手机登录</button></Demo>;
 if(step==='CLAUDE'&&authPageOpen&&!oauthAllowed)return <Demo title="演示浏览器 · Agbrio 授权页"><p>已自动登录。申请方：{request.clientName}</p><p>授权范围：本机 Agbrio，可在设置撤销。</p><button type="button" onClick={()=>{oauthAllowed=true;events.push('OAUTH_ALLOWED');notify();}}>允许并连接</button></Demo>;
 if(step==='CLAUDE'&&oauthAllowed)return <Demo title="演示 Claude 桌面应用 · Code · 新对话">{readResult?<pre>agbrio_read_app 成功{'\n'}scope: INSTANCE{'\n'}approvalMode: CONVERSATION_REVIEW{'\n'}expiresAt: 30 天后</pre>:<><p>{buffer===READ_CHECK?'已复制验证指令。':'先点底部“复制验证指令”。'}</p><button type="button" disabled={buffer!==READ_CHECK} onClick={()=>{state.claudeLoaded=true;readResult=true;events.push('PLUGIN_LOADED','MCP_READ_VERIFIED');notify();}}>粘贴并发送到演示 Code</button></>}</Demo>;
 if(step==='VERIFY')return <Demo title="演示手机 · 对话">{phoneReplied?<p className="demo-result">电脑已收到：连接测试，请回复收到{'\n'}手机已同步回复：收到。</p>:<button type="button" onClick={()=>{phoneReplied=true;events.push('PHONE_REPLY_VERIFIED');notify();}}>从演示手机发送“连接测试”</button>}</Demo>;
 return null;
}
Object.assign(window,{firstRunDemo:{events,preferences,snapshot:()=>state}});
if(params.get('scenario')==='auth-gap'){connect();state.claudeLoaded=true;state.hasGrant=true;state.devices=[{id:'demo-phone',label:'iPhone · Demo'}];oauthAllowed=true;mcpStarted=true;preferences.set('agbrio.desktop-first-run.v1',JSON.stringify({version:1,disposition:'PENDING',agents:['CLAUDE_CODE'],step:'CLAUDE_VERIFY'}));events.push('FIXTURE_AUTH_GAP');}
createRoot(document.getElementById('root')!).render(<DesktopFirstRun native api={api} preview={preview} companion={step=><Companion key={step} step={step}/>} storage={{getItem:key=>preferences.get(key)??null,setItem:(key,value)=>{preferences.set(key,value);}}}><main className="demo-finished"><h1>完整引导体验结束</h1><p>你已走完所选 Agent、手机、监听与首次跟进。这里所有连接均为演示，真实操作请使用新版客户端。</p><button onClick={()=>location.reload()}>重新体验完整引导</button></main></DesktopFirstRun>);
