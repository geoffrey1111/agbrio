import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {UnifiedWorkbench,type UnifiedWorkbenchProps} from './UnifiedWorkbench';
import type {OverviewApi} from './SettingsHome';
import {HostPiAvailability} from './HostPiAvailability';
import {ConversationHome} from './ConversationHome';
import type {NotificationApi} from './CodexNotifications';
afterEach(()=>{cleanup();localStorage.clear();});
const base:UnifiedWorkbenchProps={items:[],draft:{value:''},onDraftChange:()=>{},onSelectWorkstream:()=>{},surface:'SETTINGS'};
const mac=async()=>({hostName:'Fixture Mac',platform:'macos',features:{pi:false}});
function overview():OverviewApi{return {shared:vi.fn().mockResolvedValue({state:'UNSUPPORTED'}),pi:vi.fn().mockResolvedValue({installed:false}),devices:vi.fn().mockResolvedValue([]),update:vi.fn().mockResolvedValue({state:'UNSUPPORTED'})};}
it('Mac desktop omits Pi overview/settings and never reads Pi installation',async()=>{
 const api=overview();render(<UnifiedWorkbench {...base} computerIdentity={mac} overviewApi={api} piSettingsPanel={<div>Pi panel sentinel</div>} codexConnectionPanel={<div>Codex</div>} assistantPanel={<div>Assistant</div>}/>);
 await waitFor(()=>expect(api.shared).toHaveBeenCalled());expect(api.pi).not.toHaveBeenCalled();expect(screen.queryByText(/Pi 管理员后台|Pi panel sentinel/)).toBeNull();
 expect(screen.queryByRole('button',{name:/Pi 子任务/})).toBeNull();expect(screen.getAllByRole('button',{name:/Codex 共享连接/}).length).toBeGreaterThan(0);
});
it('Windows continues to offer Pi regardless of this browser OS',async()=>{
 render(<UnifiedWorkbench {...base} computerIdentity={async()=>({hostName:'Win',platform:'windows',features:{pi:true}})} overviewApi={overview()} piSettingsPanel={<div>Pi panel sentinel</div>}/>);
 expect(await screen.findByRole('button',{name:/Pi 管理员后台/})).toBeVisible();
});
it('phone paired to a Mac omits Pi copy using Host capability',async()=>{
 render(<UnifiedWorkbench {...base} computerIdentity={mac} assistantPanel={<div>Assistant</div>}/>);
 await waitFor(()=>expect(screen.getByText('共享 Codex 和应用更新在电脑上设置。')).toBeVisible());
 expect(screen.queryByText(/Pi 子任务/)).toBeNull();
});
it('a Mac directory does not surface a legacy Pi row or remove its Codex neighbor',async()=>{
 const watch=(threadId:string,label:string)=>({sequence:1,threadId,label,cwd:'/fixture',enabled:true,generation:1,checkedAt:1,errorCode:null,snapshot:{state:'RESULT_READY',turnId:null,itemId:null,text:''}});
 const api={watches:async()=>[watch('pi:old','Legacy Pi sentinel'),watch('codex-fixture','Keep Codex')],candidates:async()=>[]} as unknown as NotificationApi;
 render(<HostPiAvailability identity={mac}><ConversationHome bridges={[]} activity={{}} activityClock={0} watchApi={api} addRequest={0} onOpenBridge={()=>{}} onOpenWatch={()=>{}}/></HostPiAvailability>);
 expect(await screen.findByText('Keep Codex')).toBeVisible();expect(screen.queryByText('Legacy Pi sentinel')).toBeNull();
});
it('Mac add-conversation dialog retains Codex/Claude selection without Pi guidance',async()=>{
 const api={watches:async()=>[],connect:async()=>{},threads:async()=>({threads:[]})} as unknown as NotificationApi;
 const node=(addRequest:number)=><HostPiAvailability identity={mac}><ConversationHome bridges={[]} activity={{}} activityClock={0} watchApi={api} addRequest={addRequest} onOpenBridge={()=>{}} onOpenWatch={()=>{}}/></HostPiAvailability>;
 const view=render(node(0));view.rerender(node(1));
 fireEvent.click(await screen.findByRole('button',{name:/监听一个对话/}));
 await waitFor(()=>expect(screen.getByRole('searchbox',{name:'搜索标题或项目'})).toBeVisible());
 await screen.findByText(/没有找到。这里只列出/);
 expect(screen.queryByText(/Pi 子任务会出现在/)).toBeNull();expect(screen.queryByRole('alert')).toBeNull();
});
