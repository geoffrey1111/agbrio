import {StrictMode} from 'react';
import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {FirstRunWizard,READ_CHECK,phoneUrl} from './FirstRunWizard';
import {DesktopFirstRun} from './DesktopFirstRun';
import {OPEN_SETUP_EVENT,SETUP_STORAGE_KEY,readSetupPreference,setupPlan,type SetupSnapshot} from './firstRunModel';
import type {FirstRunApi} from './firstRunApi';
afterEach(cleanup);
function storage(){const values=new Map<string,string>();return {getItem:(key:string)=>values.get(key)??null,setItem:(key:string,value:string)=>{values.set(key,value);}};}
export function setupFixture(missingAddress=false){
 const state:SetupSnapshot={shared:{enabled:false,ready:false,desktopConnected:false,state:'DISABLED'},claudeLoaded:false,hasGrant:false,mcpUrl:missingAddress?null:'https://phone.fixture.invalid/mcp',origin:missingAddress?null:'https://phone.fixture.invalid',publicReady:!missingAddress,devices:[],existingUse:false,hosted:{available:true,enabled:false,state:'DISABLED',origin:null,expiresAt:null,trialAvailable:true}};
 const watched:{threadId:string;enabled:boolean}[]=[];
 const api:FirstRunApi={candidates:vi.fn(async()=>[{id:'demo-codex',label:'Codex 测试对话'},{id:'claude_code:demo',label:'Claude 测试对话'}]),watches:vi.fn(async()=>watched.slice()),watch:vi.fn(async threadId=>{watched.push({threadId,enabled:true});}),snapshot:vi.fn(async()=>({...state,devices:state.devices?.slice()})),devices:vi.fn(async()=>state.devices?.slice()),prepareCodex:vi.fn(async()=>state.shared={enabled:true,ready:true,desktopConnected:false,state:'PREPARED'}),openCodex:vi.fn(async()=>state.shared={enabled:true,ready:true,desktopConnected:true,state:'CONNECTED'}),installClaude:vi.fn(async()=>({phase:'WAITING_SESSION' as const,writesEnabled:false})),trial:vi.fn(async()=>{state.origin='https://phone.fixture.invalid';state.mcpUrl=state.origin+'/mcp';state.publicReady=true;return state.hosted={available:true,enabled:true,state:'READY',origin:state.origin,expiresAt:Date.now()+7*86400000,kind:'TRIAL',trialAvailable:false};}),redeem:vi.fn(),configure:vi.fn(),retryConnection:vi.fn(),pairCode:vi.fn(async()=>({code:'123456',expiresAt:Date.now()/1000+180})),grant:vi.fn(async()=>{state.hasGrant=true;})};return {state,api};
}
const primary=()=>document.querySelector('.first-run-primary') as HTMLButtonElement;
async function clickPrimary(){await act(async()=>{fireEvent.click(primary());});}
function oneAction(){expect(document.querySelectorAll('.first-run-primary')).toHaveLength(1);}
it('shows automatically for a new desktop profile, skips once and can reopen without remounting the workbench',async()=>{
 const s=storage(),f=setupFixture(),child=vi.fn();function Workbench(){child();return <p>Workbench ready</p>;}
 const view=render(<StrictMode><DesktopFirstRun native api={f.api} storage={s}><Workbench/></DesktopFirstRun></StrictMode>);
 expect(await screen.findByRole('dialog')).toBeVisible();expect(child).not.toHaveBeenCalled();oneAction();expect(f.api.prepareCodex).not.toHaveBeenCalled();expect(f.api.installClaude).not.toHaveBeenCalled();expect(f.api.trial).not.toHaveBeenCalled();expect(f.api.pairCode).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'跳过引导'}));expect(await screen.findByText('Workbench ready')).toBeVisible();expect(readSetupPreference(s)?.disposition).toBe('SKIPPED');view.unmount();
 render(<DesktopFirstRun native api={f.api} storage={s}><p>Existing workbench</p></DesktopFirstRun>);expect(screen.queryByRole('dialog')).toBeNull();act(()=>{window.dispatchEvent(new Event(OPEN_SETUP_EVENT));});expect(await screen.findByRole('dialog')).toBeVisible();expect(screen.getByText('Existing workbench')).toBeInTheDocument();
});
it('does not nag an existing configured installation or change its connections',async()=>{
 const f=setupFixture(),s=storage();f.state.existingUse=true;render(<DesktopFirstRun native api={f.api} storage={s}><p>Existing user</p></DesktopFirstRun>);expect(await screen.findByText('Existing user')).toBeVisible();expect(screen.queryByRole('dialog')).toBeNull();expect(readSetupPreference(s)?.disposition).toBe('EXISTING');expect(f.api.trial).not.toHaveBeenCalled();
});
it('keeps skip available when the initial native read does not return',async()=>{
 vi.useFakeTimers();try{const f=setupFixture();f.api.eligibility=()=>new Promise(()=>{});render(<DesktopFirstRun native api={f.api} storage={storage()}><p>Main</p></DesktopFirstRun>);await act(async()=>{await vi.advanceTimersByTimeAsync(3000);});expect(screen.getByRole('button',{name:'跳过引导'})).toBeVisible();fireEvent.click(screen.getByRole('button',{name:'跳过引导'}));expect(screen.getByText('Main')).toBeVisible();}finally{vi.useRealTimers();}
});
it('rejects non-HTTPS, credential-bearing and path/query-bearing phone URLs',()=>{for(const value of ['http://host.invalid','https://user:secret@host.invalid','https://host.invalid/path','https://host.invalid/?token=x','javascript:alert(1)'])expect(phoneUrl(value)).toBeNull();expect(phoneUrl('https://phone.fixture.invalid')).toBe('https://phone.fixture.invalid/mobile');});
const CLAUDE={version:1 as const,disposition:'PENDING' as const,agents:['CLAUDE_CODE' as const],step:'CLAUDE' as const};
const URL_MCP='https://phone.fixture.invalid/mcp';
function claudeFixture(){const f=setupFixture();f.state.devices=[{id:'p',label:'Phone'}];let phase='NOT_STARTED';f.api.mcpStatus=vi.fn(async()=>({phase,endpoint:URL_MCP}));f.api.setupMcp=vi.fn(async()=>{phase='AUTHORIZING';return {phase,endpoint:URL_MCP};});f.api.authorizations=vi.fn(async()=>phase==='AUTHORIZING'?[{requestId:'a'.repeat(64),clientName:'Claude Code fixture',redirectUri:'http://127.0.0.1:4567/callback',resource:URL_MCP,expiresAt:Date.now()+60000}]:[]);f.api.openAuthorization=vi.fn().mockResolvedValue(undefined);f.api.copyText=vi.fn().mockResolvedValue(undefined);return {...f,allow:()=>{phase='AUTHORIZED';}};}
it('repairs the independently missing plugin even when OAuth was already completed',async()=>{
 const f=claudeFixture();f.allow();f.state.hasGrant=true;
 render(<FirstRunWizard api={f.api} initial={f.state} preference={CLAUDE} storage={storage()} onExit={vi.fn()}/>);
 await waitFor(()=>expect(primary()).toHaveTextContent('复制验证指令'));
 await clickPrimary();expect(f.api.installClaude).toHaveBeenCalledOnce();expect(f.api.setupMcp).not.toHaveBeenCalled();expect(f.api.grant).not.toHaveBeenCalled();
 expect(f.api.copyText).toHaveBeenCalledWith(READ_CHECK);expect(primary()).toHaveTextContent('检查插件是否上线');
 await clickPrimary();expect(f.api.installClaude).toHaveBeenCalledOnce();expect(screen.getByText('连接 Claude Code')).toBeVisible();
});
it('one click installs, grants and configures Claude, then opens the exact authorization page once without codes or pasting',async()=>{
 const f=claudeFixture();render(<FirstRunWizard api={f.api} initial={f.state} preference={CLAUDE} storage={storage()} onExit={vi.fn()}/>);
 expect(primary()).toHaveTextContent('连接 Claude Code');await clickPrimary();
 expect(f.api.installClaude).toHaveBeenCalledOnce();expect(f.api.grant).toHaveBeenCalledOnce();expect(f.api.setupMcp).toHaveBeenCalledOnce();
 await waitFor(()=>expect(f.api.openAuthorization).toHaveBeenCalledOnce());expect(f.api.pairCode).not.toHaveBeenCalled();expect(f.api.copyText).not.toHaveBeenCalled();
 expect(await screen.findByText(/核对申请方后点击“允许并连接”/)).toBeVisible();
 f.allow();await waitFor(()=>expect(screen.getByText('官方登录已完成')).toBeVisible(),{timeout:4000});
 expect(primary()).toHaveTextContent('复制验证指令');await clickPrimary();expect(f.api.copyText).toHaveBeenCalledWith(READ_CHECK);
 expect(primary()).toHaveTextContent('检查插件是否上线');await clickPrimary();expect(screen.getByRole('alert')).toHaveTextContent('还没有收到插件上线');
 f.state.claudeLoaded=true;await clickPrimary();expect(screen.getByRole('heading',{name:'选择要在手机上跟进的对话'})).toBeVisible();expect(f.api.openAuthorization).toHaveBeenCalledOnce();
},10000);
it('reuses an existing OAuth login and only asks for the Code-session read check',async()=>{
 const f=claudeFixture();f.allow();f.state.hasGrant=true;f.state.claudeLoaded=true;render(<FirstRunWizard api={f.api} initial={f.state} preference={CLAUDE} storage={storage()} onExit={vi.fn()}/>);
 await waitFor(()=>expect(primary()).toHaveTextContent('复制验证指令'));expect(f.api.setupMcp).not.toHaveBeenCalled();expect(f.api.installClaude).not.toHaveBeenCalled();
});
it('asks the owner to pick between several authorization requests instead of opening one',async()=>{
 const f=claudeFixture();vi.mocked(f.api.authorizations!).mockResolvedValue([1,2].map(n=>({requestId:String(n).repeat(64),clientName:`Claude ${n}`,redirectUri:'http://127.0.0.1/cb',resource:URL_MCP,expiresAt:Date.now()+60000})));
 render(<FirstRunWizard api={f.api} initial={f.state} preference={CLAUDE} storage={storage()} onExit={vi.fn()}/>);await clickPrimary();
 await waitFor(()=>expect(primary()).toHaveTextContent('选择本次连接后继续'));expect(f.api.openAuthorization).not.toHaveBeenCalled();
 fireEvent.click(screen.getByLabelText(/Claude 2/));await clickPrimary();expect(f.api.openAuthorization).toHaveBeenCalledWith('2'.repeat(64));
});
it('keeps a differently configured agbrio MCP server instead of overwriting it',async()=>{
 const f=claudeFixture();vi.mocked(f.api.setupMcp!).mockRejectedValue(Error('CLAUDE_MCP_DIFFERENT_SERVER_PRESERVED'));render(<FirstRunWizard api={f.api} initial={f.state} preference={CLAUDE} storage={storage()} onExit={vi.fn()}/>);await clickPrimary();
 expect(screen.getByRole('alert')).toHaveTextContent('已保留而未覆盖');
});
it('gives a usable fallback when copying the read check fails',async()=>{
 const f=claudeFixture();f.allow();f.state.hasGrant=true;vi.mocked(f.api.copyText!).mockRejectedValue(Error('clipboard denied'));render(<FirstRunWizard api={f.api} initial={f.state} preference={CLAUDE} storage={storage()} onExit={vi.fn()}/>);
 await waitFor(()=>expect(primary()).toHaveTextContent('复制验证指令'));await clickPrimary();
 expect((screen.getByRole('textbox',{name:'手动复制读取验证指令'}) as HTMLTextAreaElement).value).toBe(READ_CHECK);
});
it('prepares and opens shared Codex in one action, and never relaunches an unconfirmed window',async()=>{
 const f=setupFixture();vi.mocked(f.api.openCodex).mockResolvedValue({enabled:true,ready:true,desktopConnected:false,state:'UNKNOWN'});render(<FirstRunWizard api={f.api} initial={f.state} preference={{version:1,disposition:'PENDING',agents:['CODEX'],step:'CODEX'}} storage={storage()} onExit={vi.fn()}/>);
 await clickPrimary();expect(f.api.prepareCodex).toHaveBeenCalledOnce();expect(f.api.openCodex).toHaveBeenCalledOnce();expect(primary()).toHaveTextContent('检查当前状态');
 await clickPrimary();expect(f.api.openCodex).toHaveBeenCalledOnce();expect(screen.getByRole('alert')).toHaveTextContent('不会重复打开');
});
it('explains how to quit ordinary Codex in place and retries only after the owner acts',async()=>{
 const f=setupFixture();vi.mocked(f.api.openCodex).mockRejectedValueOnce(Error('SHARED_DESKTOP_CLOSE_REQUIRED'));render(<FirstRunWizard api={f.api} initial={f.state} preference={{version:1,disposition:'PENDING',agents:['CODEX'],step:'CODEX'}} storage={storage()} onExit={vi.fn()}/>);
 await clickPrimary();expect(screen.getByText('请先退出普通 Codex')).toBeVisible();expect(primary()).toHaveTextContent('已退出，打开共享 Codex');
 await clickPrimary();expect(f.api.openCodex).toHaveBeenCalledTimes(2);expect(screen.getByRole('heading',{name:'连接你的手机'})).toBeVisible();
});
it('Codex-only setup pairs the phone automatically and never installs Claude or grants MCP access',async()=>{
 const f=setupFixture(),exit=vi.fn();render(<FirstRunWizard api={f.api} storage={storage()} onExit={exit}/>);fireEvent.click(screen.getByLabelText(/Codex共享/));await clickPrimary();
 expect(screen.getByRole('heading',{name:'连接 Codex'})).toBeVisible();await clickPrimary();
 expect(screen.getByRole('heading',{name:'连接你的手机'})).toBeVisible();await waitFor(()=>expect(f.api.pairCode).toHaveBeenCalledOnce());expect(await screen.findByText('123456')).toBeVisible();
 f.state.devices=[{id:'phone',label:'iPhone'}];await waitFor(()=>expect(primary()).toHaveTextContent('继续'),{timeout:5000});await clickPrimary();
 expect(screen.getByRole('heading',{name:'选择要在手机上跟进的对话'})).toBeVisible();expect(await screen.findByLabelText('Codex 测试对话')).toBeChecked();expect(screen.queryByLabelText('Claude 测试对话')).toBeNull();
 await clickPrimary();expect(f.api.watch).toHaveBeenCalledWith('demo-codex');await clickPrimary();await clickPrimary();expect(exit).toHaveBeenCalledWith('COMPLETED',['CODEX']);
 expect(f.api.installClaude).not.toHaveBeenCalled();expect(f.api.grant).not.toHaveBeenCalled();
},10000);
it('needs an explicit trial choice, then shows the QR code on the same screen',async()=>{
 const f=setupFixture(true);render(<FirstRunWizard api={f.api} initial={f.state} preference={{version:1,disposition:'PENDING',agents:['CLAUDE_CODE'],step:'PHONE'}} storage={storage()} onExit={vi.fn()}/>);
 expect(f.api.trial).not.toHaveBeenCalled();expect(primary()).toHaveTextContent('开启 7 天免费试用');await clickPrimary();
 expect(f.api.trial).toHaveBeenCalledOnce();expect(f.api.pairCode).toHaveBeenCalledOnce();expect(screen.getByRole('img',{name:'手机扫码打开 Agbrio'})).toBeVisible();expect(screen.getByText('123456')).toBeVisible();
});
it('preserves goal order and performs a read-only inspection before any uncertain retry',async()=>{
 expect(setupPlan(['CLAUDE_CODE','CODEX'])).toEqual(['CHOOSE','CODEX','PHONE','CLAUDE','WATCH','VERIFY','DONE']);
 const f=setupFixture();vi.mocked(f.api.prepareCodex).mockRejectedValue(Error('SHARED_CONNECTION_UNCERTAIN'));render(<FirstRunWizard api={f.api} storage={storage()} onExit={vi.fn()}/>);fireEvent.click(screen.getByLabelText(/Codex共享/));await clickPrimary();await clickPrimary();expect(f.api.prepareCodex).toHaveBeenCalledOnce();
 f.state.shared={enabled:false,ready:false,desktopConnected:false,state:'UNKNOWN'};await clickPrimary();expect(f.api.prepareCodex).toHaveBeenCalledOnce();expect(screen.getByRole('alert')).toHaveTextContent('不会重复启动');
 f.state.shared={enabled:false,ready:false,desktopConnected:false,state:'DISABLED'};await clickPrimary();expect(screen.getByRole('alert')).toHaveTextContent('下一次点击将重新准备');expect(f.api.prepareCodex).toHaveBeenCalledOnce();
});
it('resumes an older saved step on its goal screen and never stores the one-use pairing code',async()=>{
 const f=setupFixture(),s=storage();s.setItem(SETUP_STORAGE_KEY,JSON.stringify({version:1,disposition:'PENDING',agents:['CODEX'],step:'PHONE_PAIR'}));expect(readSetupPreference(s)?.step).toBe('PHONE');
 render(<DesktopFirstRun native api={f.api} storage={s}><p>Main</p></DesktopFirstRun>);expect(await screen.findByRole('heading',{name:'连接你的手机'})).toBeVisible();await waitFor(()=>expect(f.api.pairCode).toHaveBeenCalled());
 expect(s.getItem(SETUP_STORAGE_KEY)).not.toContain('123456');expect(f.api.openCodex).not.toHaveBeenCalled();
});
it('offers optional home-screen guidance after pairing and a fresh sign-in code without retaining it',async()=>{
 const f=setupFixture(),s=storage();f.state.devices=[{id:'phone',label:'iPhone'}];render(<FirstRunWizard api={f.api} initial={f.state} preference={{version:1,disposition:'PENDING',agents:['CODEX'],step:'PHONE'}} storage={s} onExit={vi.fn()}/>);
 expect(screen.getByText(/已收到手机登录/)).toBeVisible();expect(f.api.pairCode).not.toHaveBeenCalled();fireEvent.click(screen.getByText('可选：添加到手机桌面并开启通知'));
 await act(async()=>{fireEvent.click(screen.getByRole('button',{name:'生成新的手机登录码'}));});expect(f.api.pairCode).toHaveBeenCalledOnce();expect(screen.getByText('123456')).toBeVisible();expect(s.getItem(SETUP_STORAGE_KEY)).not.toContain('123456');
});
it('watches one exact chat per agent in one action and reconciles an uncertain watch before any retry',async()=>{
 const f=setupFixture();vi.mocked(f.api.watch!).mockRejectedValue(Error('WATCH_UNCONFIRMED'));render(<FirstRunWizard api={f.api} initial={f.state} preference={{version:1,disposition:'PENDING',agents:['CODEX','CLAUDE_CODE'],step:'WATCH'}} storage={storage()} onExit={vi.fn()}/>);
 expect(await screen.findByLabelText('Codex 测试对话')).toBeChecked();expect(screen.getByLabelText('Claude 测试对话')).toBeChecked();
 await clickPrimary();expect(f.api.watch).toHaveBeenCalledOnce();await clickPrimary();expect(f.api.watch).toHaveBeenCalledOnce();expect(screen.getByRole('alert')).toHaveTextContent('不会自动重复执行');
});
it('does not claim another trial after an uncertain activation, including after reopening',async()=>{
 const f=setupFixture(true),s=storage();vi.mocked(f.api.trial).mockRejectedValue(Error('HOSTED_ACTIVATION_UNKNOWN'));const pref={version:1 as const,disposition:'PENDING' as const,agents:['CODEX' as const],step:'PHONE' as const};
 const view=render(<FirstRunWizard api={f.api} initial={f.state} preference={pref} storage={s} onExit={vi.fn()}/>);await clickPrimary();expect(f.api.trial).toHaveBeenCalledOnce();expect(readSetupPreference(s)?.needsCheck).toBe('PHONE');await clickPrimary();expect(f.api.trial).toHaveBeenCalledOnce();view.unmount();
 render(<FirstRunWizard api={f.api} initial={f.state} preference={readSetupPreference(s)} storage={s} onExit={vi.fn()}/>);await clickPrimary();expect(f.api.trial).toHaveBeenCalledOnce();
});
it('serializes rapid clicks on a mutating action',async()=>{
 const f=setupFixture();let finish!:(s:NonNullable<SetupSnapshot['shared']>)=>void;vi.mocked(f.api.prepareCodex).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));render(<FirstRunWizard api={f.api} storage={storage()} onExit={vi.fn()}/>);fireEvent.click(screen.getByLabelText(/Codex共享/));await clickPrimary();
 const button=primary();act(()=>{fireEvent.click(button);fireEvent.click(button);});await waitFor(()=>expect(f.api.prepareCodex).toHaveBeenCalledOnce());expect(screen.getByRole('button',{name:'跳过引导'})).toBeDisabled();
 await act(async()=>{finish({enabled:true,ready:true,desktopConnected:false,state:'PREPARED'});});expect(f.api.prepareCodex).toHaveBeenCalledOnce();
});
