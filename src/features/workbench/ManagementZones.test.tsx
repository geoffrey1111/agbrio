import {cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {HostPiAvailability} from './HostPiAvailability';
import {ManagementZones,constellation,tierOf} from './ManagementZones';
import type {ManagementSnapshot,ManagementApi,ManagedConversation} from './managementZoneApi';
afterEach(()=>{cleanup();sessionStorage.clear();});
const row=(id:string,label:string,provider:ManagedConversation['provider']='CODEX'):ManagedConversation=>({threadId:id,label,provider,cwd:'D:/project',generation:3,state:'RUNNING',checkedAt:Date.now(),available:true,watchEnabled:true});
function fixture(){
 const first=row('claude_code:old','原管理','CLAUDE_CODE'),second=row('new','新管理'),source=row('exec','执行对话');
 const data:ManagementSnapshot={zones:[{id:'a',name:'美术制作',managerId:first.threadId,manager:first,revision:7,updatedAt:1,members:[{threadId:source.threadId,generation:3,conversation:source}]},{id:'b',name:'工程开发',managerId:second.threadId,manager:second,revision:5,updatedAt:1,members:[]}],candidates:[first,second,source],legacyAssignments:[]};
 const api:ManagementApi={read:vi.fn(async()=>structuredClone(data)),change:vi.fn(async()=>({}))};
 render(<ManagementZones api={api} onOpenWatch={vi.fn()}/>);return{api,data};
}
it('shows exact manager and executor hierarchy; secondary actions stay off the first screen',async()=>{
 fixture();expect(await screen.findByRole('button',{name:/美术制作/,pressed:true})).toBeVisible();
 const manager=screen.getByRole('button',{name:/管理对话.*原管理/});expect(manager).toBeVisible();
 expect(screen.getByRole('button',{name:/执行对话.*执行对话/})).toBeVisible();
 expect(screen.queryByRole('button',{name:'转移'})).toBeNull();expect(screen.queryByRole('button',{name:'移出'})).toBeNull();
 fireEvent.click(manager);expect(screen.getByText(/分工不改变原对话权限/)).toBeVisible();
});
it('switches to an emphasis-sized constellation when executors grow',()=>{
 const rows=Array.from({length:10},(_,i)=>row(`e${i}`,`执行 ${i}`));rows[7].state='ACTION_REQUIRED';rows[3].state='IDLE';
 const {placed,rings}=constellation(rows);expect(rings).toBe(2);expect(placed).toHaveLength(10);
 expect(placed[0].row.threadId).toBe('e7');expect(placed[0].ring).toBe(0);expect(tierOf(rows[3])).toBe('QUIET');
 expect(new Set(placed.map(p=>p.row.threadId)).size).toBe(10);
 for(const p of placed){expect(p.x).toBeGreaterThanOrEqual(0);expect(p.x).toBeLessThanOrEqual(100);}
});
it('appoints a Codex manager with one revision-bound owner operation',async()=>{
 const{api}=fixture();fireEvent.click(await screen.findByRole('button',{name:'管理区操作'}));fireEvent.click(screen.getByRole('menuitem',{name:'更换管理对话'}));
 fireEvent.click(screen.getByRole('radio',{name:/新管理/}));
 fireEvent.click(screen.getByRole('button',{name:'确认更换'}));
 await waitFor(()=>expect(api.change).toHaveBeenCalledOnce());
 expect(vi.mocked(api.change).mock.calls[0][0]).toMatchObject({action:'APPOINT',zoneId:'a',revision:7,managerId:'new'});
 expect(screen.queryByText('取消订阅')).toBeNull();
});
it('transfers with exact original zone and both revisions, never remove then add',async()=>{
 const{api}=fixture();fireEvent.click(await screen.findByRole('button',{name:/执行对话.*执行对话/}));fireEvent.click(screen.getByRole('button',{name:'转移'}));
 expect(screen.getByText(/不会先移出再加入/)).toBeVisible();
 fireEvent.click(within(screen.getByRole('radiogroup',{name:'转移到'})).getByRole('radio',{name:/工程开发/}));
 fireEvent.click(screen.getByRole('button',{name:'确认转移'}));
 await waitFor(()=>expect(api.change).toHaveBeenCalledOnce());
 expect(vi.mocked(api.change).mock.calls[0][0]).toMatchObject({action:'MOVE',zoneId:'b',revision:5,fromZoneId:'a',fromRevision:7,threadId:'exec',generation:3});
});
it('reconciles uncertain operations using the same request ID',async()=>{
 const{api}=fixture();vi.mocked(api.change).mockRejectedValueOnce(new Error('NETWORK')).mockResolvedValueOnce({});
 fireEvent.click(await screen.findByRole('button',{name:/管理对话.*原管理/}));fireEvent.click(screen.getByRole('button',{name:'更换管理对话'}));fireEvent.click(screen.getByRole('radio',{name:/新管理/}));fireEvent.click(screen.getByRole('button',{name:'确认更换'}));
 fireEvent.click(await screen.findByRole('button',{name:'检查并完成这次操作'}));
 await waitFor(()=>expect(api.change).toHaveBeenCalledTimes(2));
 expect(vi.mocked(api.change).mock.calls[1][0]).toEqual(vi.mocked(api.change).mock.calls[0][0]);
});
it('adopts existing subscriptions as one operation instead of a chain of partial changes',async()=>{
 const{api,data}=fixture();data.legacyAssignments=[{managerId:'claude_code:old',threadId:'standalone'}];
 fireEvent.click(await screen.findByRole('button',{name:'管理区操作'}));fireEvent.click(screen.getByRole('menuitem',{name:'刷新管理区'}));
 fireEvent.click(await screen.findByRole('button',{name:/原管理.*1.*条执行对话/}));
 const form=screen.getByRole('form',{name:'接管现有分工'});
 fireEvent.click(within(form).getByRole('button',{name:'确认'}));
 await waitFor(()=>expect(api.change).toHaveBeenCalledOnce());
 expect(vi.mocked(api.change).mock.calls[0][0]).toMatchObject({action:'ADOPT',managerId:'claude_code:old',threadIds:['standalone']});
});
it('lets the owner switch between normal and constellation views and remembers the device choice',async()=>{
 localStorage.removeItem('agbrio.management.view');fixture();await screen.findByRole('button',{name:/美术制作/,pressed:true});
 fireEvent.click(screen.getByRole('button',{name:'普通视图'}));expect(document.querySelector('.mz-tree')).not.toBeNull();expect(localStorage.getItem('agbrio.management.view')).toBe('NORMAL');
 fireEvent.click(screen.getByRole('button',{name:'星图视图'}));expect(document.querySelector('.mz-map')).not.toBeNull();expect(screen.getByRole('button',{name:/执行对话.*执行对话/})).toBeVisible();
 localStorage.removeItem('agbrio.management.view');
});

it('Mac keeps manager and constellation views while excluding Pi members and choices',async()=>{
 const manager=row('manager','Mac manager'),codex=row('codex','Mac executor'),pi=row('pi:old','Legacy Pi','PI');
 const data:ManagementSnapshot={zones:[{id:'mac',name:'Mac zone',managerId:manager.threadId,manager,revision:1,updatedAt:1,members:[codex,pi].map(conversation=>({threadId:conversation.threadId,generation:3,conversation}))}],candidates:[manager,codex,pi],legacyAssignments:[]};
 render(<HostPiAvailability identity={async()=>({platform:'macos',hostName:'Fixture',features:{pi:false}})}><ManagementZones api={{read:async()=>data,change:vi.fn()}} onOpenWatch={vi.fn()}/></HostPiAvailability>);
 expect(await screen.findByRole('button',{name:/Mac zone/})).toBeVisible();
 expect(await screen.findByRole('button',{name:/执行对话.*Mac executor/})).toBeVisible();
 expect(screen.queryByText('Legacy Pi')).toBeNull();expect(document.body.textContent).not.toMatch(/Pi/);
 fireEvent.click(screen.getByRole('button',{name:'星图视图'}));
 expect(screen.getByRole('button',{name:/执行对话.*Mac executor/})).toBeVisible();expect(screen.queryByText('Legacy Pi')).toBeNull();
});
