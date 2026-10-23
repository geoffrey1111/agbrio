import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {ConversationHome} from './ConversationHome';
import type {CodexWatch,NotificationApi} from './CodexNotifications';
afterEach(cleanup);
const watch=(id:string,at:number,state='RESULT_READY'):CodexWatch=>({threadId:id,label:id,cwd:'C:/fixture',lastMessageAt:at,checkedAt:Date.now(),enabled:true,generation:1,errorCode:null,snapshot:{state,turnId:'turn',itemId:'message',text:'fixture'}});
function fixture(initial:CodexWatch[]){let rows=initial;let notify=()=>{};const api={connect:vi.fn(),threads:vi.fn(),enable:vi.fn(),pause:vi.fn(),feed:vi.fn(),event:vi.fn(),webUrl:vi.fn(),cachedWatches:()=>rows,watches:vi.fn(async()=>rows),subscribe:(listener:()=>void)=>{notify=listener;return()=>{};}} as NotificationApi;return{api,push:(next:CodexWatch[])=>{rows=next;notify();}};}
const order=()=>[...document.querySelectorAll('.v6-list .v6-row strong')].map(n=>n.textContent);
it('keeps running first, then attention, then other conversations, newest message within each layer',async()=>{
 const a=fixture([watch('new-completed',900),watch('old-running',100,'RUNNING'),watch('recent-running',200,'RUNNING'),watch('claude_code:waiting',700,'ACTION_REQUIRED')]);
 render(<ConversationHome bridges={[{id:'bridge',name:'middle-bridge',lifecycle:'ACTIVE',updatedAt:999999,lastMessageAt:800}]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);
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
