import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {ConversationHome} from './ConversationHome';
import type {CodexWatch,NotificationApi} from './CodexNotifications';
afterEach(cleanup);
const watch=(id:string,at:number,state='RESULT_READY'):CodexWatch=>({threadId:id,label:id,cwd:'C:/fixture',lastMessageAt:at,checkedAt:Date.now(),enabled:true,generation:1,errorCode:null,snapshot:{state,turnId:'turn',itemId:'message',text:'fixture'}});
function fixture(initial:CodexWatch[]){let rows=initial;let notify=()=>{};const api={connect:vi.fn(),threads:vi.fn(),enable:vi.fn(),pause:vi.fn(),feed:vi.fn(),event:vi.fn(),webUrl:vi.fn(),cachedWatches:()=>rows,watches:vi.fn(async()=>rows),subscribe:(listener:()=>void)=>{notify=listener;return()=>{};}} as NotificationApi;return{api,push:(next:CodexWatch[])=>{rows=next;notify();}};}
const order=()=>[...document.querySelectorAll('.v6-list .v6-row strong')].map(n=>n.textContent);
it('merges Bridge, Claude and Codex by latest message without status or edit-time buckets',async()=>{
 const a=fixture([watch('new-completed',300),watch('old-running',100,'RUNNING'),watch('claude_code:waiting',200,'ACTION_REQUIRED')]);
 render(<ConversationHome bridges={[{id:'bridge',name:'middle-bridge',lifecycle:'ACTIVE',updatedAt:999999,lastMessageAt:250}]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);
 await waitFor(()=>expect(order()).toEqual(['new-completed','middle-bridge','claude_code:waiting','old-running']));expect(screen.queryByText('其他')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'执行中1'}));await waitFor(()=>expect(order()).toEqual(['old-running']));
});
it('moves a conversation only on a new message and preserves that order through search and remount',async()=>{
 const a=fixture([watch('a',200),watch('b',100)]);const props={bridges:[],activity:{},activityClock:Date.now(),watchApi:a.api,addRequest:0,onOpenBridge:vi.fn(),onOpenWatch:vi.fn()};const view=render(<ConversationHome {...props}/>);
 await waitFor(()=>expect(order()).toEqual(['a','b']));act(()=>a.push([{...watch('a',200),checkedAt:999999,sendCount:999},watch('b',300)]));expect(order()).toEqual(['b','a']);
 fireEvent.change(screen.getByRole('searchbox'),{target:{value:'a'}});await waitFor(()=>expect(order()).toEqual(['a']));fireEvent.change(screen.getByRole('searchbox'),{target:{value:''}});expect(order()).toEqual(['b','a']);view.unmount();render(<ConversationHome {...props}/>);expect(order()).toEqual(['b','a']);
});
it('keeps unknown timestamps last and breaks equal times by stable identity',async()=>{
 const a=fixture([watch('z',200),watch('unknown',0,'ACTION_REQUIRED'),watch('a',200)]);
 render(<ConversationHome bridges={[]} activity={{}} activityClock={Date.now()} watchApi={a.api} addRequest={0} onOpenBridge={vi.fn()} onOpenWatch={vi.fn()}/>);await waitFor(()=>expect(order()).toEqual(['a','z','unknown']));
});
