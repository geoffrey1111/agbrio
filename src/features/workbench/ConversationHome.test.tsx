import {act,cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {ConversationHome} from './ConversationHome';
import type {CodexWatch,NotificationApi} from './CodexNotifications';
import {QUIET_WATCH_AGE} from './watchPresentation';
afterEach(cleanup);
const messageEpoch=Date.now()-60000;
const watch=(id:string,at:number,state='RESULT_READY'):CodexWatch=>({threadId:id,label:id,cwd:'C:/fixture',lastMessageAt:at?messageEpoch+at:0,checkedAt:Date.now(),enabled:true,generation:1,errorCode:null,snapshot:{state,turnId:'turn',itemId:'message',text:'fixture'}});
function fixture(initial:CodexWatch[]){let rows=initial;let notify=()=>{};const api={connect:vi.fn(),threads:vi.fn(),enable:vi.fn(),pause:vi.fn(),feed:vi.fn(),event:vi.fn(),webUrl:vi.fn(),cachedWatches:()=>rows,watches:vi.fn(async()=>rows),show:vi.fn(async(id:string)=>{const shownAt=Date.now();rows=rows.map(w=>w.threadId===id?{...w,shownAt}:w);notify();return{shownAt};}),subscribe:(listener:()=>void)=>{notify=listener;return()=>{};}} as NotificationApi;return{api,push:(next:CodexWatch[])=>{rows=next;notify();}};}
const order=()=>[...document.querySelectorAll('.v6-list .v6-row strong')].map(n=>n.textContent);
it('folds Pi under its native manager, promotes child activity and retains an all-Pi lookup',async()=>{
 const parent=watch('claude_code:manager',1);
 const child={...watch('pi:child',3,'RUNNING'),piParent:{provider:'CLAUDE_CODE',externalId:'manager',cwd:parent.cwd}};
 const a=fixture([parent,child,watch('ordinary',2)]),open=vi.fn();
 render(<ConversationHome bridges={[]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={open}/>);
 await screen.findByText('Pi 子任务执行中 · 1 项');expect(order()).toEqual(['claude_code:manager','ordinary']);
 fireEvent.click(screen.getByRole('button',{name:'Pi 子任务 · 1 项'}));expect(screen.getByText('pi:child')).toBeVisible();
 fireEvent.click(screen.getByText('pi:child'));expect(open).toHaveBeenCalledWith(child);
 fireEvent.click(screen.getByRole('button',{name:'Pi'}));await waitFor(()=>expect(order()).toEqual(['pi:child']));
});
it('folds three-day quiet watches without pausing and resurfaces actual new messages',async()=>{
 const old={...watch('quiet',1),lastMessageAt:Date.now()-QUIET_WATCH_AGE-1000};const a=fixture([old,watch('running',1,'RUNNING')]);
 render(<ConversationHome bridges={[]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);
 await waitFor(()=>expect(screen.getByText('较早的对话')).toBeVisible());expect(screen.getByText('quiet')).not.toBeVisible();expect(screen.getByText('running')).toBeVisible();expect(a.api.pause).not.toHaveBeenCalled();
 act(()=>a.push([{...old,checkedAt:Date.now()+999999},watch('running',1,'RUNNING')]));expect(screen.getByText('quiet')).not.toBeVisible();
 act(()=>a.push([{...old,lastMessageAt:Date.now()},watch('running',1,'RUNNING')]));await waitFor(()=>expect(screen.getByText('quiet')).toBeVisible());expect(screen.queryByText('较早的对话')).toBeNull();
});
it('lets search reveal an old watch and an explicit show restore it without enabling or pausing',async()=>{
 const a=fixture([{...watch('quiet',1),lastMessageAt:Date.now()-QUIET_WATCH_AGE-1000}]);
 render(<ConversationHome bridges={[]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);
 await screen.findByText('较早的对话');fireEvent.change(screen.getByRole('searchbox'),{target:{value:'quiet'}});expect(screen.getByText('quiet')).toBeVisible();expect(a.api.show).not.toHaveBeenCalled();
 fireEvent.change(screen.getByRole('searchbox'),{target:{value:''}});fireEvent.click(screen.getByText('较早的对话'));fireEvent.click(screen.getByRole('button',{name:'移回最近：quiet'}));
 await waitFor(()=>expect(screen.queryByText('较早的对话')).toBeNull());expect(a.api.show).toHaveBeenCalledWith('quiet');expect(a.api.enable).not.toHaveBeenCalled();expect(a.api.pause).not.toHaveBeenCalled();
});
it('re-adding an already watched quiet conversation resurfaces the exact identity only',async()=>{
 const old={...watch('quiet',1),lastMessageAt:Date.now()-QUIET_WATCH_AGE-1000};const a=fixture([old]);
 vi.mocked(a.api.threads).mockResolvedValue({threads:[{id:'quiet',label:'quiet',updatedAt:'1'}]} as never);const onOpenWatch=vi.fn();
 const props={bridges:[],activity:{},activityClock:Date.now(),watchApi:a.api,onOpenBridge:vi.fn(),onOpenWatch};const view=render(<ConversationHome {...props} addRequest={0}/>);view.rerender(<ConversationHome {...props} addRequest={1}/>);
 fireEvent.click(await screen.findByText('监听一个对话'));
 const dialog=await screen.findByRole('dialog');fireEvent.click(await within(dialog).findByText('quiet'));
 await waitFor(()=>expect(a.api.show).toHaveBeenCalledWith('quiet'));expect(a.api.enable).not.toHaveBeenCalled();await waitFor(()=>expect(onOpenWatch).toHaveBeenCalledWith(expect.objectContaining({threadId:'quiet',shownAt:expect.any(Number)})));
});
it('can still select Claude when connecting the Codex transport fails',async()=>{
 const a=fixture([]);const id='claude_code:10000000-0000-4000-8000-000000000021';vi.mocked(a.api.connect).mockRejectedValue(new Error('Codex backend disconnected'));vi.mocked(a.api.threads).mockResolvedValue({threads:[{id,label:'selected Claude',updatedAt:'1'}]} as never);
 render(<ConversationHome bridges={[]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);fireEvent.click(screen.getByRole('button',{name:'开始'}));fireEvent.click(await screen.findByText('监听一个对话'));
 const dialog=await screen.findByRole('dialog');fireEvent.click(await within(dialog).findByText('selected Claude'));await waitFor(()=>expect(a.api.enable).toHaveBeenCalledWith(id));expect(screen.queryByText('没有开始监听，请重试。')).toBeNull();
});
it('does not report an acknowledged watch write as failed when its follow-up list read is offline',async()=>{
 const a=fixture([]);vi.mocked(a.api.threads).mockResolvedValue({threads:[{id:'exact',label:'selected',updatedAt:'1'}]} as never);a.api.refreshWatches=vi.fn().mockRejectedValue(new Error('offline'));
 render(<ConversationHome bridges={[]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);fireEvent.click(screen.getByRole('button',{name:'开始'}));fireEvent.click(await screen.findByText('监听一个对话'));const dialog=await screen.findByRole('dialog');fireEvent.click(await within(dialog).findByText('selected'));
 await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());expect(a.api.enable).toHaveBeenCalledOnce();
});
it('keeps running first, then attention, then other conversations, newest message within each layer',async()=>{
 const a=fixture([watch('new-completed',900),watch('old-running',100,'RUNNING'),watch('recent-running',200,'RUNNING'),watch('claude_code:waiting',700,'ACTION_REQUIRED')]);
 render(<ConversationHome bridges={[{id:'bridge',name:'middle-bridge',lifecycle:'ACTIVE',updatedAt:Date.now()+1000,lastMessageAt:messageEpoch+800}]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);
 await waitFor(()=>expect(order()).toEqual(['recent-running','old-running','claude_code:waiting','new-completed','middle-bridge']));
 const section=screen.getByText('其他');expect(section.previousElementSibling).toHaveTextContent('claude_code:waiting');expect(section.nextElementSibling).toHaveTextContent('new-completed');
 act(()=>a.push([watch('new-completed',1000),watch('old-running',300,'RUNNING'),watch('recent-running',200,'RUNNING'),watch('claude_code:waiting',700,'ACTION_REQUIRED')]));expect(order()).toEqual(['old-running','recent-running','claude_code:waiting','new-completed','middle-bridge']);
 fireEvent.click(screen.getByRole('button',{name:'执行中2'}));await waitFor(()=>expect(order()).toEqual(['old-running','recent-running']));await waitFor(()=>expect(screen.queryByText('其他')).toBeNull());
});
it('moves a finished conversation into others and a newly running conversation to the top',async()=>{
 const a=fixture([watch('runner',100,'RUNNING'),watch('new-result',900)]);
 render(<ConversationHome bridges={[]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);
 await waitFor(()=>expect(order()).toEqual(['runner','new-result']));
 act(()=>a.push([watch('runner',100),watch('new-result',900,'RUNNING')]));expect(order()).toEqual(['new-result','runner']);expect(screen.getByText('其他')).toBeVisible();
 act(()=>a.push([watch('runner',100),watch('new-result',900)]));await waitFor(()=>expect(screen.getByText('最近')).toBeVisible());expect(order()).toEqual(['new-result','runner']);
});
it('moves a conversation only on a new message and preserves that order through search and remount',async()=>{
 const a=fixture([watch('a',200),watch('b',100)]);const props={bridges:[],activity:{},activityClock:Date.now(),watchApi:a.api,addRequest:0,onOpenBridge:vi.fn(),onOpenWatch:vi.fn()};const view=render(<ConversationHome {...props}/>);
 await waitFor(()=>expect(order()).toEqual(['a','b']));act(()=>a.push([{...watch('a',200),checkedAt:999999,sendCount:999},watch('b',300)]));expect(order()).toEqual(['b','a']);
 fireEvent.change(screen.getByRole('searchbox'),{target:{value:'a'}});await waitFor(()=>expect(order()).toEqual(['a']));fireEvent.change(screen.getByRole('searchbox'),{target:{value:''}});expect(order()).toEqual(['b','a']);view.unmount();render(<ConversationHome {...props}/>);expect(order()).toEqual(['b','a']);
});
it('keeps unknown timestamps last and breaks equal times by stable identity',async()=>{
 const a=fixture([watch('z',200),watch('unknown',0),watch('a',200)]);
 render(<ConversationHome bridges={[]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);await waitFor(()=>expect(order()).toEqual(['a','z','unknown']));
});
