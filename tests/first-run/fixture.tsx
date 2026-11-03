import {createRoot} from 'react-dom/client';
import {useSyncExternalStore} from 'react';
import {DesktopFirstRun} from '../../src/features/onboarding/DesktopFirstRun';
import type {FirstRunPreview} from '../../src/features/onboarding/FirstRunWizard';
import type {FirstRunApi} from '../../src/features/onboarding/firstRunApi';
import type {SetupSnapshot,SetupStep} from '../../src/features/onboarding/firstRunModel';
import {setLanguagePreference} from '../../src/i18n';
import '../../src/styles/agbrio-v6-theme.css';
import '../../src/styles/agbrio-v7.css';
import '../../src/styles/agbrio-v7-r2.css';
import './fixture.css';
setLanguagePreference(new URLSearchParams(location.search).get('lang')==='en'?'en':'zh-CN');
const state:SetupSnapshot={existingUse:false,shared:{enabled:false,ready:false,desktopConnected:false,state:'DISABLED'},claudeLoaded:false,hasGrant:false,mcpUrl:null,origin:null,publicReady:false,devices:[],hosted:{available:true,enabled:false,state:'DISABLED',origin:null,expiresAt:null,trialAvailable:true}};
const events:string[]=[],preferences=new Map<string,string>(),watched:{threadId:string;enabled:boolean}[]=[];
let issued=0,lastCode='',mcpAdded=false,oauthAllowed=false,buffer='',authCodeAvailable=false,readResult=false,phoneReplied=false,managerReady=false,revision=0;
const drafts:Record<string,string>={},listeners=new Set<()=>void>();let demoMessage='';function notify(){revision++;listeners.forEach(fn=>fn());}const subscribe=(fn:()=>void)=>{listeners.add(fn);return()=>{listeners.delete(fn);};};
function connect(origin='https://my-computer.fixture.invalid') {state.origin=origin;state.mcpUrl=origin+'/mcp';state.publicReady=true;return state.hosted={available:true,enabled:true,state:'READY',origin,expiresAt:Date.now()+7*86400000,kind:'TRIAL',trialAvailable:false};}
const api:FirstRunApi={setupMcp:async()=>{mcpAdded=true;events.push('MCP_CONFIGURED');return {phase:'AUTHORIZING',endpoint:state.mcpUrl!};},mcpStatus:async()=>({phase:oauthAllowed?'AUTHORIZED':'AUTHORIZING',endpoint:state.mcpUrl!}),copyText:async text=>{buffer=text;notify();},
 snapshot:async()=>({...state,devices:state.devices?.slice()}),devices:async()=>state.devices?.slice(),
 prepareCodex:async()=>{events.push('CODEX_PREPARE');return state.shared={enabled:true,ready:true,desktopConnected:false,state:'PREPARED'};},
 openCodex:async()=>{events.push('CODEX_OPEN');return state.shared={enabled:true,ready:true,desktopConnected:true,state:'CONNECTED'};},
 installClaude:async()=>{events.push('CLAUDE_INSTALL');return {phase:'WAITING_SESSION',writesEnabled:false};},
 trial:async()=>{events.push('TRIAL');return connect();},redeem:async()=>{events.push('REDEEM');return connect();},configure:async origin=>{events.push('CUSTOM_ADDRESS');connect(origin);},retryConnection:async()=>{},
 pairCode:async()=>{events.push('PAIR_CODE');issued++;lastCode=issued===1?'123456':'654321';drafts.PHONE_PAIR=lastCode;drafts.CLAUDE_AUTH=lastCode;notify();return {code:lastCode,expiresAt:Date.now()/1000+180};},
 grant:async()=>{events.push('GRANT');state.hasGrant=true;},
 candidates:async()=>[{id:'demo-codex-chat',label:'Codex · 我的第一个任务',projectLabel:'示例项目'},{id:'claude_code:demo-manager',label:'Claude Code · 我的管理助手',projectLabel:'示例项目'}],
 watches:async()=>watched.slice(),watch:async threadId=>{events.push('WATCH:'+threadId);watched.push({threadId,enabled:true});},
};
const preview:FirstRunPreview={verified:step=>step==='CLAUDE_AUTH'?oauthAllowed:step==='CLAUDE_VERIFY'?readResult:step==='PHONE_VERIFY'?phoneReplied:false,
 description:step=>({
 CODEX_PREPARE:'本页模拟 Windows 授权和共享后台准备。点击底部按钮即可，不会操作你真实的 Codex。',
 CODEX_OPEN:'实际使用时，需要先结束任务并退出普通 Codex。本预览点击底部按钮就会模拟共享启动，无需关闭真实窗口。',
 CLAUDE_INSTALL:'点击底部按钮，模拟安装 Claude 插件。本预览不会安装或改动真实插件。',
 CLAUDE_LOAD:'这里代表 Claude 桌面应用里的原 Code 对话。点击底部按钮，模拟执行 /reload-plugins 并检查连接。',
 PHONE_PAIR:'演示配对码已自动填入。直接点击底部登录即可，不用手动抄写，也不要用真实手机扫描演示二维码。',
 CLAUDE_MCP:'点击底部，模拟 Agbrio 自动配置本机 Claude Code 的 MCP 并打开授权页。不用复制指令，不用填写地址。',
 CLAUDE_CONNECT:'这一步模拟在 Claude 桌面应用的 Code 对话粘贴发送。点底部「粘贴并发送到演示 Code」即可，无需切换真实窗口。下方三步是正式使用时的操作位置。',
 CLAUDE_AUTH:'点击底部打开演示授权页，系统会自动登录，不用输入或复制验证码。看到本次连接后，再由你点击允许。正式客户端也按这个流程打开真实授权页。',
 CLAUDE_VERIFY:'点击底部自动检查演示连接。缺少的演示授权会在本页补齐，然后显示读取结果；成功后点继续即可，不用退回找步骤。',
 PHONE_VERIFY:'测试消息已准备好，也可以修改。点击底部模拟发送，看到电脑收到和手机回复的结果后，继续即可。',
 CLAUDE_USAGE:'先用底部按钮复制只读起步指令，再点击底部模拟粘贴发送。看到管理助手读取进度的回复后，即可完成。'
 } as Partial<Record<SetupStep,string>>)[step],
 label:(step,c)=>step==='CLAUDE_MCP'?'自动配置 MCP 并打开演示授权页':step==='CLAUDE_LOAD'?'在演示 Code 中加载插件':step==='PHONE_PAIR'?'登录演示手机':step==='CLAUDE_CONNECT'?'粘贴并发送到演示 Code':step==='CLAUDE_AUTH'?authCodeAvailable?'允许本次演示连接':'打开演示授权页并自动登录':step==='CLAUDE_VERIFY'?!readResult?'自动检查并补全演示连接':'连接正常，继续':step==='PHONE_VERIFY'?!phoneReplied?'从演示手机发送':'已看到回复，继续':step==='CLAUDE_USAGE'&&c.copied?!managerReady?'粘贴并发送起步指令（演示）':'已看到管理助手读取进度，完成':undefined,
 run:async(step,c)=>{
  demoMessage='';
  if(step==='CLAUDE_LOAD'){state.claudeLoaded=true;events.push('PLUGIN_LOADED');}
  if(step==='PHONE_PAIR'){if(drafts[step]!==lastCode){demoMessage='请把本页配对码填入演示手机登录框，再点击底部按钮。';notify();return 'HANDLED';}state.devices=[{id:'demo-phone',label:'iPhone · Demo'}];events.push('PHONE_PAIRED');}
  if(step==='CLAUDE_CONNECT'){if(!buffer.includes(state.mcpUrl??'invalid')){demoMessage='还没有复制接入指令，请返回上一步重新复制。';notify();return 'HANDLED';}mcpAdded=true;events.push('MCP_ADDED');}
  if(step==='CLAUDE_AUTH'){if(!authCodeAvailable){await api.pairCode();authCodeAvailable=true;notify();return 'HANDLED';}mcpAdded=true;oauthAllowed=true;events.push('OAUTH_ALLOWED');}
  if(step==='CLAUDE_VERIFY'&&!readResult){if(!mcpAdded){mcpAdded=true;events.push('MCP_ADDED');}if(!oauthAllowed){await api.pairCode();oauthAllowed=true;events.push('OAUTH_RECOVERED');}readResult=true;events.push('MCP_READ_VERIFIED');notify();return 'HANDLED';}
  if(step==='PHONE_VERIFY'&&!phoneReplied){if(!drafts[step]?.trim())drafts[step]='连接测试，请回复收到';phoneReplied=true;events.push('PHONE_REPLY_VERIFIED');notify();return 'HANDLED';}
  if(step==='CLAUDE_USAGE'&&c.copied&&!managerReady){managerReady=true;events.push('MANAGER_READY');notify();return 'HANDLED';}
  notify();return 'CONTINUE';
 }
};
function Companion({step}:{step:SetupStep}) {
 useSyncExternalStore(subscribe,()=>revision);
 const field=(label:string)=><label>{label}<input readOnly={step==='PHONE_PAIR'||step==='CLAUDE_AUTH'} value={drafts[step]??(step==='PHONE_VERIFY'?'连接测试，请回复收到':'')} onChange={e=>{drafts[step]=e.target.value;notify();}}/></label>;
 let content;
 if(step==='CLAUDE_MCP')content=<p>正式客户端由 Agbrio 调用本机官方 Claude Code 完成配置。本预览只模拟这条实际流程，不会改动真实客户端。</p>;
 if(step==='CLAUDE_CONNECT')content=<><p>演示窗口：Claude 桌面应用 → Code → 我的管理助手。</p><p>点击底部「粘贴并发送到演示 Code」，整段指令会发送到这里，不需要切换真实应用。</p><details><summary>查看即将发送的完整指令</summary><pre>{buffer}</pre></details></>;
 if(step==='CLAUDE_LOAD')content=<p>演示 Code 已打开原对话。点击底部按钮执行 /reload-plugins，再检查插件连接。</p>;
 if(step==='PHONE_PAIR')content=<><p>演示手机已扫码打开 Agbrio 登录页。配对码已自动填好，直接点击底部「登录演示手机」。</p>{field('演示手机的配对码')}</>;
 if(step==='CLAUDE_AUTH')content=<><p>演示浏览器已打开 Agbrio 授权页。</p>{!authCodeAvailable?<p>点击底部即可自动打开授权页并登录，不需要抄码。</p>:<><p>已自动登录演示授权页。</p><p>申请方：Claude Code · 我的管理助手</p><p>授权范围：本机 Agbrio，可在设置撤销。</p><p>请点击底部「允许本次演示连接」。这一步由你决定。</p></>}</>;
 if(step==='CLAUDE_VERIFY')content=readResult?<><p>演示 Code 的工具读取结果：</p><pre>agbrio_read_app 成功{'\n'}scope: INSTANCE{'\n'}approvalMode: CONVERSATION_REVIEW{'\n'}expiresAt: 30 天后{'\n'}本机：演示电脑 · 示例项目</pre><p>已自动确认演示连接成功。点击底部「连接正常，继续」。</p></>:<p>点击底部「自动检查并补全演示连接」，系统会检查授权并显示读取结果。</p>;
 if(step==='PHONE_VERIFY')content=<><p>演示手机已显示：{watched.map(row=>row.threadId.startsWith('claude_code:')?'Claude 管理助手':'Codex 测试对话').join('、')}。</p>{!phoneReplied?<>{field('演示手机消息')}<p>已准备好测试消息，可直接点击底部发送。</p></>:<div className="demo-result"><p>电脑已收到：{drafts[step]}</p><p>手机已同步助手回复：收到。</p><p>模拟收发已验证，直接点击底部继续。</p></div>}</>;
 if(step==='CLAUDE_USAGE')content=managerReady?<p>演示管理助手回复：已读到示例项目中的 Codex 和 Claude 对话状态。准备就绪，等待你交代任务。</p>:<p>复制起步指令后，点击底部发送到同一个演示 Code 对话。</p>;
 if(!content)return step==='CHOOSE'?<p className="demo-notice">完整交互预览：跟着每页底部按钮操作。演示外部应用会显示在本页，不需要真的切换窗口。</p>:null;
 return <section className="demo-companion" aria-label="外部应用模拟"><strong>演示窗口 · 按本页底部按钮操作</strong>{content}{demoMessage&&<p role="alert" className="demo-result">{demoMessage}</p>}</section>;
}
Object.assign(window,{firstRunDemo:{events,preferences,loadClaude:()=>{state.claudeLoaded=true;},pairPhone:()=>{state.devices=[{id:'demo-phone',label:'iPhone · Demo'}];},snapshot:()=>state}});
if(new URLSearchParams(location.search).get('scenario')==='auth-gap'){connect();state.claudeLoaded=true;state.hasGrant=true;mcpAdded=true;preferences.set('agbrio.desktop-first-run.v1',JSON.stringify({version:1,disposition:'PENDING',agents:['CLAUDE_CODE'],step:'CLAUDE_VERIFY'}));events.push('FIXTURE_AUTH_GAP');}
createRoot(document.getElementById('root')!).render(<DesktopFirstRun native api={api} preview={preview} companion={step=><Companion key={step} step={step}/>} storage={{getItem:key=>preferences.get(key)??null,setItem:(key,value)=>{preferences.set(key,value);}}}><main className="demo-finished"><h1>完整引导体验结束</h1><p>你已走完所选 Agent、手机、监听与首次跟进。这里所有连接均为演示，真实操作请使用新版客户端。</p><button onClick={()=>location.reload()}>重新体验完整引导</button></main></DesktopFirstRun>);
