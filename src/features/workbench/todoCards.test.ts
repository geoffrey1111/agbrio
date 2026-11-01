import {expect,it} from 'vitest';
import type {CodexWatch} from './CodexNotifications';
import type {WorkbenchItem} from './models';
import {deriveTodoCards} from './todoCards';
import {buildChildTree,childTreeSummary} from './childTree';

const now=Date.now();
const watch=(id:string,state:string,extra:Partial<CodexWatch>={}):CodexWatch=>({threadId:id,label:id,cwd:'C:/fixture',lastMessageAt:now-1000,checkedAt:now,enabled:true,generation:1,errorCode:null,snapshot:{state,turnId:'turn',itemId:'item',text:'latest preview'},...extra});
const none=()=>false;
const derive=(watches:CodexWatch[],bridges:WorkbenchItem[]=[],activity={})=>deriveTodoCards({watches,bridges,activity,activityClock:now,isDismissed:none});

it('uses exact native pending requests when the Host projects them, one card per question with its own text',()=>{
 const w=watch('claude_code:parent','ACTION_REQUIRED',{pending:[{requestId:'r1',kind:'QUESTION',prompt:'选 A 还是 B？',childId:null,generation:1,revision:3,canRespond:true,since:now-50},{requestId:'r2',kind:'PERMISSION',prompt:null,childId:'kid',generation:1,revision:1,canRespond:true,since:now-10}],taskTree:{nodes:[{sessionId:'kid',parentSessionId:'parent',title:'子任务',depth:1,state:'WAITING_DECISION',updatedAt:now-10}],running:0,attention:1,unknown:0,limited:false}});
 const cards=derive([w]);
 expect(cards.map(c=>[c.group,c.title,c.text,c.childId])).toEqual([['ANSWER','子任务','有一项权限请求等你确认',  'kid'],['ANSWER','claude_code:parent','选 A 还是 B？',undefined]]);
});
it('falls back to the coarse waiting state and native WAITING nodes before T2 is deployed',()=>{
 const w=watch('claude_code:parent','ACTION_REQUIRED',{taskTree:{nodes:[{sessionId:'kid',parentSessionId:'parent',title:'子任务',depth:1,state:'WAITING_REPLY',updatedAt:now}],running:0,attention:1,unknown:0,limited:false}});
 const cards=derive([w]);
 expect(cards.map(c=>c.text)).toEqual(['子对话等你选择','latest preview']);
 expect(derive([watch('done','RESULT_READY',{unreadResult:true})])).toEqual([]);
});
it('counts only current-binding handoffs, and failures until acknowledged',()=>{
 const bridge:WorkbenchItem={id:'b1',name:'Bridge 一',lifecycle:'ACTIVE'};
 const row={workstreamId:'b1',bindingRevision:2,sides:[],readyHandoffs:[{id:'h1',sourceObservationId:'o1',sourceRole:'EXECUTION',status:'READY' as const,bindingRevision:2},{id:'h0',sourceObservationId:'o0',sourceRole:'EXECUTION',status:'READY' as const,bindingRevision:1}]};
 const cards=derive([watch('pi:failed','FAILED')],[bridge],{b1:{row,receivedAt:now}});
 expect(cards.map(c=>[c.group,c.text])).toEqual([['SEND','1 项结果等你审阅后转发'],['FAILED','latest preview']]);
 expect(deriveTodoCards({watches:[watch('pi:failed','FAILED')],bridges:[],activity:{},activityClock:now,isDismissed:()=>true})).toEqual([]);
});
it('folds finished children into the ended group, caps indentation input and keeps needs-you first',()=>{
 const old=now-2*86400000;
 const tree={nodes:[{sessionId:'a',parentSessionId:'root',title:'A',depth:1,state:'TURN_ENDED',updatedAt:old},{sessionId:'b',parentSessionId:'a',title:'B',depth:2,state:'TURN_ENDED',updatedAt:old},{sessionId:'c',parentSessionId:'root',title:'C',depth:1,state:'WAITING_DECISION',updatedAt:old},{sessionId:'d',parentSessionId:'c',title:'D',depth:2,state:'RUNNING',updatedAt:now},{sessionId:'e',parentSessionId:'d',title:'E',depth:3,state:'RUNNING',updatedAt:now},{sessionId:'f',parentSessionId:'e',title:'F',depth:4,state:'RUNNING',updatedAt:now}],running:3,attention:1,unknown:0,limited:false};
 const pi=[watch('pi:closed','RESULT_READY',{lastMessageAt:old-1,piLifecycle:{state:'CLOSED',loaded:false,closedAt:now}})];
 const m=buildChildTree({parentThreadId:'claude_code:root',tree,pi,now,isDismissed:none,seen:none});
 expect(m.active.map(e=>[e.title,e.depth])).toEqual([['C',1],['D',2],['E',3],['F',4]]);
 expect(m.ended.map(e=>[e.title,e.label])).toEqual([['A','本轮已结束'],['B','本轮已结束'],['pi:closed','已关闭']]);
 expect(childTreeSummary(m)).toBe('子对话 7 · 等你 1 · 执行中 3');
});
it('shows an upstream account problem as needing the owner, first in the to-do and in the tree, not as unreadable',()=>{
 const billing=watch('pi:billing','FAILED',{errorCode:'PI_PROVIDER_BILLING_REQUIRED',snapshot:{state:'FAILED',turnId:'t',itemId:'i',text:'credit balance is too low'}});
 const cards=derive([watch('codex:ask','ACTION_REQUIRED'),billing]);
 expect(cards.map(c=>[c.group,c.text,c.detail])).toEqual([['OWNER','Pi 的上游 API 余额不足','credit balance is too low'],['ANSWER','latest preview',undefined]]);
 const m=buildChildTree({parentThreadId:'claude_code:root',tree:null,pi:[billing],now,isDismissed:none,seen:none});
 expect(m.active.map(e=>[e.tone,e.label])).toEqual([['wait','需要你处理']]);expect(m.unknown).toBe(0);
});
