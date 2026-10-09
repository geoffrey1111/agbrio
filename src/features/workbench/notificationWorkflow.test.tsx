import{act,cleanup,fireEvent,render,screen,within}from'@testing-library/react';
import{afterEach,expect,it,vi}from'vitest';
import{CodexNotifications,type CodexWatch,type NotificationApi}from'./CodexNotifications';
import{RoleBridgePanel,type RoleState}from'./RoleBridgePanel';
import{NativeSurfaceContext}from'./NativeSurfaceContext';
afterEach(()=>{cleanup();vi.useRealTimers();localStorage.clear();});
const watch=(id:string,count:number):CodexWatch=>({threadId:id,label:id,cwd:'D:\\fictional',enabled:true,generation:1,snapshot:{state:'IDLE',turnId:null,itemId:null,text:'fictional current message'},checkedAt:Date.now(),errorCode:null,sendCount:count});
function api(rows:CodexWatch[]):NotificationApi{return{watches:vi.fn().mockResolvedValue(rows),connect:vi.fn(),threads:vi.fn(),enable:vi.fn(),pause:vi.fn(),feed:vi.fn().mockResolvedValue({events:[],nextCursor:0,hasMore:false}),event:vi.fn(),webUrl:vi.fn().mockResolvedValue(null)};}
const order=()=>[...document.querySelectorAll<HTMLElement>('[data-watch-id]')].map(n=>n.dataset.watchId);
it('freezes usage order across polls and an opened chat; leaving WATCHES recalculates cumulative counts',async()=>{
 vi.useFakeTimers();const a=api([watch('less-used',2),watch('most-used',8)]);
 render(<CodexNotifications api={a} standalone/>);await act(async()=>{});fireEvent.click(screen.getByRole('tab',{name:'监听对话'}));expect(order()).toEqual(['most-used','less-used']);
 const open=within(document.querySelector('[data-watch-id="less-used"]') as HTMLElement).getByRole('button',{name:'查看当前内容'});fireEvent.click(open);
 vi.mocked(a.watches).mockResolvedValue([watch('less-used',40),watch('most-used',8)]);await act(async()=>{await vi.advanceTimersByTimeAsync(5100);});fireEvent.click(screen.getByRole('button',{name:'关闭内容'}));expect(order()).toEqual(['most-used','less-used']);
 fireEvent.click(screen.getByRole('tab',{name:'最近通知'}));fireEvent.click(screen.getByRole('tab',{name:'监听对话'}));expect(order()).toEqual(['less-used','most-used']);
});
it('loads watched rows independently of a stalled recent feed and distinguishes pending reads from an empty list',async()=>{
 const a=api([]);let ready!:(rows:CodexWatch[])=>void;vi.mocked(a.watches).mockImplementation(()=>new Promise(resolve=>{ready=resolve;}));vi.mocked(a.feed).mockImplementation(()=>new Promise(()=>{}));
 render(<CodexNotifications api={a} standalone/>);fireEvent.click(screen.getByRole('tab',{name:'监听对话'}));expect(screen.getByText('正在读取…')).toBeVisible();expect(screen.queryByText('选择后会展示当前内容；后续变化进入最近通知。')).toBeNull();
 await act(async()=>ready([watch('loaded-watch',3)]));expect(document.querySelector('[data-watch-id="loaded-watch"]')).toBeVisible();expect(order()).toEqual(['loaded-watch']);expect(screen.queryByText('正在读取…')).toBeNull();
});
it('keeps list order within the whole page visit and refreshes only after leaving the notifications page',async()=>{
 vi.useFakeTimers();const a=api([watch('b',1),watch('a',1)]);const view=(active:boolean)=><div className='v3-workbench-main'><CodexNotifications api={a} workbenchPage={{active,open:vi.fn()}}/></div>;
 const{rerender}=render(view(true));await act(async()=>{});fireEvent.click(screen.getByRole('tab',{name:'监听对话'}));expect(order()).toEqual(['a','b']);
 vi.mocked(a.watches).mockResolvedValue([watch('b',30),watch('a',1)]);await act(async()=>{await vi.advanceTimersByTimeAsync(5100);});expect(order()).toEqual(['a','b']);rerender(view(false));rerender(view(true));expect(order()).toEqual(['b','a']);
});
it('replaces source latest attention with a visible receiver execution signal while preserving the source text',async()=>{
 const now=Date.now();const state:RoleState={bindings:{workstreamId:'fictional',bindingRevision:1,explicitRoles:true,decision:{role:'DECISION',endpoint:{id:'d',provider:'CODEX',externalId:'decision-thread',label:'control'}},execution:{role:'EXECUTION',endpoint:{id:'e',provider:'CODEX',externalId:'execution-thread',label:'executor'}}},replies:[{id:'source',endpointId:'d',text:'retained result',observedAt:now-1000,completedAt:now-1000,handledAt:now-500}],handoffs:[],activities:[{role:'DECISION',endpointId:'d',state:'COMPLETE',resultObservationId:'source',checkedAt:now},{role:'EXECUTION',endpointId:'e',state:'RUNNING',turnActive:true,checkedAt:now}],snapshotAt:now};
 const a:any={state:vi.fn().mockResolvedValue(state),sync:vi.fn().mockResolvedValue(state)};render(<NativeSurfaceContext.Provider value><RoleBridgePanel workstreamId='fictional' api={a}/></NativeSurfaceContext.Provider>);
 await screen.findByText('retained result');await screen.findByText('正在执行');expect(document.querySelectorAll('.r2-latest-dot')).toHaveLength(0);expect(document.querySelector('.r2-latest-reply')).toBeNull();expect(screen.getByText('已转发')).toBeVisible();expect(document.querySelector('.r2-bridge-active-status')).toHaveTextContent('执行');
});
