import {act,cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {ManagementZones,constellation,tierOf} from './ManagementZones';
import type {ManagementSnapshot,ManagementApi,ManagedConversation} from './managementZoneApi';
afterEach(()=>{cleanup();sessionStorage.clear();});
const row=(id:string,label:string,provider:ManagedConversation['provider']='CODEX'):ManagedConversation=>({threadId:id,label,provider,cwd:'D:/project',generation:3,state:'RUNNING',checkedAt:Date.now(),available:true,watchEnabled:true});
function fixture(beforeRender?:(data:ManagementSnapshot)=>void){
 const first=row('claude_code:old','原管理','CLAUDE_CODE'),second=row('new','新管理'),source=row('exec','执行对话');
 const data:ManagementSnapshot={zones:[{id:'a',name:'美术制作',managerId:first.threadId,manager:first,revision:7,updatedAt:1,members:[{threadId:source.threadId,generation:3,conversation:source}]},{id:'b',name:'工程开发',managerId:second.threadId,manager:second,revision:5,updatedAt:1,members:[]}],candidates:[first,second,source],legacyAssignments:[]};
 const api:ManagementApi={read:vi.fn(async()=>structuredClone(data)),change:vi.fn(async()=>({}))};
 beforeRender?.(data);
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
it('reads fresh titles on manual refresh and shows the returned manager name',async()=>{
 const {api,data}=fixture();
 await screen.findByRole('button',{name:/管理对话.*原管理/});
 let resolve!:(snapshot:ManagementSnapshot)=>void;
 vi.mocked(api.read).mockImplementationOnce(()=>new Promise(r=>{resolve=r;}));
 fireEvent.click(screen.getByRole('button',{name:'管理区操作'}));
 fireEvent.click(screen.getByRole('menuitem',{name:'刷新管理区'}));
 expect(api.read).toHaveBeenLastCalledWith(true);
 expect(screen.getByText('正在刷新对话名称…')).toBeVisible();
 const renamed=structuredClone(data);
 renamed.zones[0].manager!.label='管理区新标题';
 resolve(renamed);
 expect(await screen.findByRole('button',{name:/管理对话.*管理区新标题/})).toBeVisible();
 expect(screen.getByText('已重新读取管理区和对话名称。')).toBeVisible();
 expect(api.change).not.toHaveBeenCalled();
});
it('rechecks immediately after an interrupted read when the page has already returned',async()=>{
 const {api,data}=fixture();
 await screen.findByRole('button',{name:/管理对话.*原管理/});
 let reject!:(reason:unknown)=>void;
 vi.mocked(api.read).mockImplementationOnce(()=>new Promise((_,fail)=>{reject=fail;}));
 act(()=>window.dispatchEvent(new Event('pageshow')));
 expect(api.read).toHaveBeenCalledTimes(2);
 const next=structuredClone(data);next.zones[0].manager!.label='恢复后的标题';
 vi.mocked(api.read).mockResolvedValueOnce(next);
 await act(async()=>{
  window.dispatchEvent(new Event('pageshow'));
  window.dispatchEvent(new Event('online'));
  reject(new DOMException('ROUTER_READ_SUSPENDED','AbortError'));
 });
 expect(screen.getByRole('button',{name:/管理对话.*恢复后的标题/})).toBeVisible();
 expect(api.read).toHaveBeenCalledTimes(3);
 expect(api.change).not.toHaveBeenCalled();
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

it('deletes a populated area in one revision-bound operation with a clear preservation note',async()=>{
 const {api}=fixture();fireEvent.click(await screen.findByRole('button',{name:'管理区操作'}));fireEvent.click(screen.getByRole('menuitem',{name:'删除管理区'}));
 const form=screen.getByRole('form',{name:'删除管理区'});expect(within(form).getByText(/原对话和正在运行的任务保留/)).toBeVisible();fireEvent.click(within(form).getByRole('button',{name:'删除管理区'}));
 await waitFor(()=>expect(api.change).toHaveBeenCalledOnce());expect(vi.mocked(api.change).mock.calls[0][0]).toMatchObject({action:'DELETE',zoneId:'a',revision:7});
});
it('defaults adoption to the existing area of the same manager and preserves its revision',async()=>{
 const {api,data}=fixture();data.legacyAssignments=[{managerId:'claude_code:old',threadId:'standalone'}];
 fireEvent.click(await screen.findByRole('button',{name:'管理区操作'}));fireEvent.click(screen.getByRole('menuitem',{name:'刷新管理区'}));fireEvent.click(await screen.findByRole('button',{name:/原管理.*1.*条执行对话/}));
 const form=screen.getByRole('form',{name:'接管现有分工'});expect(within(form).getByRole('combobox',{name:'选择管理区'})).toHaveValue('a');fireEvent.click(within(form).getByRole('button',{name:'确认'}));
 await waitFor(()=>expect(api.change).toHaveBeenCalledOnce());expect(vi.mocked(api.change).mock.calls[0][0]).toMatchObject({action:'ADOPT',zoneId:'a',revision:7,managerId:'claude_code:old',threadIds:['standalone']});
});

it('refreshes on opening the manager picker and finds a new Claude child by title or either ID',async()=>{
 const{api,data}=fixture();
 fireEvent.click(await screen.findByRole('button',{name:'管理区操作'}));
 const native='10000000-0000-4000-8000-000000000001';
 const alias='local_20000000-0000-4000-8000-000000000002';
 data.candidates.push({...row('claude_code:'+native,'新管理子对话（接手 11-13）','CLAUDE_CODE'),localSessionId:alias,watchEnabled:false});
 fireEvent.click(screen.getByRole('menuitem',{name:'更换管理对话'}));
 const query=screen.getByRole('searchbox');
 for(const value of ['11-13',native,alias]){
  fireEvent.change(query,{target:{value}});
  expect(await screen.findByRole('radio',{name:/接手 11-13/})).toBeVisible();
 }
 fireEvent.click(screen.getByRole('radio',{name:/接手 11-13/}));
 fireEvent.click(screen.getByRole('button',{name:'确认更换'}));
 await waitFor(()=>expect(api.change).toHaveBeenCalledWith(expect.objectContaining({managerId:'claude_code:'+native,revision:7})));
});

it('shows only current executors in both views, and restores members on a new turn',async()=>{
 vi.useFakeTimers();
 try{
  localStorage.setItem('agbrio.management.view','MAP');
  const ended={...row('ended','已结束对话'),state:'RESULT_READY',lastActivityAt:Date.now()-4*86400000};
  const running=row('running','长期运行对话');
  const thinking={...row('thinking','思考对话'),state:'THINKING'};
  const inactive=[ended,{...row('unreadable','读取失败对话'),available:false},{...row('question','等待确认对话'),state:'ACTION_REQUIRED'},{...row('stopped','中断对话'),state:'INCOMPLETE'}];
  const {api}=fixture(data=>{
   data.zones[0].members=[running,thinking,...inactive].map(conversation=>({threadId:conversation.threadId,generation:3,conversation}));
   data.zones[0].members.push({threadId:'missing',generation:3,conversation:null});
   data.zones[1].members=[{threadId:'external',generation:3,conversation:row('external','其他区对话')}];
  });
  await act(async()=>{});
  const find=(label:string)=>screen.queryByRole('button',{name:new RegExp('执行对话.*'+label)});
  for(const label of ['长期运行对话','思考对话'])expect(find(label)).toBeVisible();
  for(const label of [...inactive.map(member=>member.label),'其他区对话'])expect(find(label)).toBeNull();
  expect(screen.queryByRole('button',{name:/不活跃对话/})).toBeNull();
  expect(within(screen.getByRole('button',{name:/美术制作/})).getByLabelText('2 条正在执行')).toBeVisible();
  fireEvent.click(screen.getByRole('button',{name:'普通视图'}));
  expect(find('长期运行对话')).toBeVisible();
  fireEvent.click(find('长期运行对话')!);
  running.state='RESULT_READY';
  thinking.state='IDLE';
  ended.state='RUNNING';
  await act(async()=>{await vi.advanceTimersByTimeAsync(5000);});
  expect(find('已结束对话')).toBeVisible();
  expect(find('长期运行对话')).toBeNull();
  expect(screen.queryByRole('complementary',{name:'对话详情'})).toBeNull();
  vi.mocked(api.read).mockRejectedValueOnce(new Error('DISCONNECTED'));
  await act(async()=>{await vi.advanceTimersByTimeAsync(5000);});
  expect(find('已结束对话')).toBeNull();
  expect(screen.getByText('暂时无法读取管理区，正在自动重试。')).toBeVisible();
  await act(async()=>{await vi.advanceTimersByTimeAsync(5000);});
  expect(find('已结束对话')).toBeVisible();
  ended.state='IDLE';
  await act(async()=>{await vi.advanceTimersByTimeAsync(5000);});
  expect(screen.getByText('当前没有正在执行的对话')).toBeVisible();
  expect(api.change).not.toHaveBeenCalled();
 }finally{vi.useRealTimers();localStorage.removeItem('agbrio.management.view');}
});
it('keeps open-conversation failures in the inspector and permits retry',async()=>{
 const conversation=row('manager','管理对话');
 const data:ManagementSnapshot={zones:[{id:'zone',name:'验收区',managerId:'manager',manager:conversation,revision:1,updatedAt:1,members:[]}],candidates:[conversation],legacyAssignments:[]};
 const api:ManagementApi={read:async()=>data,change:vi.fn()};
 const watch={threadId:'manager',label:'管理对话'};
 const watchApi={watches:vi.fn().mockRejectedValueOnce(new Error('NETWORK')).mockResolvedValueOnce([watch])} as unknown as import('./CodexNotifications').NotificationApi;
 const open=vi.fn();
 render(<ManagementZones api={api} watchApi={watchApi} onOpenWatch={open}/>);
 fireEvent.click(await screen.findByRole('button',{name:/管理对话.*管理对话/}));
 const inspector=screen.getByRole('complementary',{name:'对话详情'});
 fireEvent.click(within(inspector).getByRole('button',{name:'打开对话'}));
 expect(await within(inspector).findByRole('alert')).toHaveTextContent('操作未完成');
 fireEvent.click(within(inspector).getByRole('button',{name:'打开对话'}));
 await waitFor(()=>expect(open).toHaveBeenCalledExactlyOnceWith(watch));
 expect(within(inspector).queryByRole('alert')).toBeNull();
});

it('calls an expired active observation unconfirmed until the watcher reports back',async()=>{
 const {data}=fixture(snapshot=>{snapshot.zones[0].members[0].conversation!.checkedAt=Date.now()-91_000;});
 expect(await screen.findByText('1 条对话的执行状态待确认，正在重试。')).toBeVisible();
 expect(screen.queryByRole('button',{name:/执行对话.*执行对话/})).toBeNull();
 expect(screen.queryByText('当前没有正在执行的对话')).toBeNull();
 data.zones[0].members[0].conversation!.checkedAt=Date.now();
 await act(async()=>{window.dispatchEvent(new Event('online'));});
 expect(await screen.findByRole('button',{name:/执行对话.*执行对话/})).toBeVisible();
});
