import {expect,it} from 'vitest';
import {chatTimeline,chronologicalHistoryPage,mergeHistory} from './chatTimeline';
import type {ChatMessage,WatchReply} from './watchChatApi';
const m=(turnId:string,id:string,role:'user'|'assistant',text=id):ChatMessage=>({turnId,id,role,text});
const source={...m('t2','a2','assistant'),seenAt:200};
const receipt=(id:string,turnId:string|null,text:string,createdAt:number,status:WatchReply['status']='SENT'):WatchReply=>({id,turnId,text,createdAt,status,threadId:'exact',sourceSequence:2,expectedTurnId:null,mode:'SEND',errorCode:null,options:{attachments:[]}});
it('reverses native turn groups while keeping consecutive public items in native order',()=>{
 const page=chronologicalHistoryPage([m('t2','u2','user'),m('t2','p2','assistant'),m('t2','a2','assistant'),m('t1','u1','user'),m('t1','a1','assistant')]);
 expect(page.map(x=>x.id)).toEqual(['u1','a1','u2','p2','a2']);
 expect(mergeHistory([m('t0','u0','user')],page).map(x=>x.id)).toEqual(['u0','u1','a1','u2','p2','a2']);
});
it('interleaves native prompts and results instead of repeating all local receipts below the latest result',()=>{
 const history=[m('t1','u1','user','prompt 1'),m('t1','a1','assistant'),m('t2','u2','user','prompt 2'),source];
 const result=chatTimeline(history,[source],[receipt('r2','t2','prompt 2',150),receipt('r1','t1','prompt 1',50)],source,true);
 expect(result.map(x=>x.id)).toEqual(['u1','a1','u2','a2']);expect(result.filter(x=>x.receipt)).toHaveLength(2);
});
it('decorates the acknowledged native initiating prompt when its body includes an attachment manifest',()=>{
 const reply=receipt('r','t2','prompt',100);
 const history=[m('t2','u2','user','prompt\n\nSelected attachments: report.md (SHA-256: demo)'),source];
 const result=chatTimeline(history,[],[reply],source,true);
 expect(result.map(x=>x.id)).toEqual(['u2','a2']);expect(result[0].receipt?.id).toBe('r');
 expect(result[0].text).toContain('Selected attachments');
});
it('retains repeated same-text native user messages and assigns each receipt once',()=>{
 const history=[m('t2','u2','user','again'),m('t2','p2','assistant'),m('t2','u3','user','again'),source];
 const result=chatTimeline(history,[],[receipt('r1','t2','again',100),receipt('r2','t2','again',150)],source,true);
 expect(result.map(x=>x.id)).toEqual(['u2','p2','u3','a2']);expect(result[0].receipt?.id).toBe('r1');expect(result[2].receipt?.id).toBe('r2');
});
it('uses fresh native content over an older cache, then advances the exact live item',()=>{
 const old={...source,text:'cached partial'};
 const native={...source,text:'native final'};
 expect(chatTimeline([native],[old],[],old,true)[0].text).toBe('native final');
 expect(chatTimeline([native],[old],[],old,true,{...native,text:'current live update'})[0].text).toBe('current live update');
});
it('does not stack old sent receipts outside the loaded window, but keeps uncertain/queued sends visible',()=>{
 const replies=[receipt('old','t0','old',1),receipt('unknown',null,'check delivery',250,'UNKNOWN'),receipt('queue',null,'next',300,'QUEUED')];
 const result=chatTimeline([source],[],replies,source,true);
 expect(result.map(x=>x.id)).toEqual(['a2','receipt:unknown','receipt:queue']);
 const older=chatTimeline([m('t0','u0','user','old'),m('t0','a0','assistant'),source],[],replies,source,true);
 expect(older[0].receipt?.id).toBe('old');
});

it('puts a newly observed current watched turn after the previous native window',()=>{
 const current={...m('new','report','assistant'),seenAt:300};
 const rows=chatTimeline([m('old','prompt','user'),m('old','answer','assistant')],[current],[],current,true);
 expect(rows.map(m=>m.id)).toEqual(['prompt','answer','report']);
});
it('a refreshed native prompt comes before an already cached report in that turn',()=>{
 const merged=mergeHistory([m('t','report','assistant')],[m('t','prompt','user'),m('t','report','assistant'),m('t','next','assistant')]);
 expect(merged.map(m=>m.id)).toEqual(['prompt','report','next']);
});

it('a dispatched queued reply with native material text decorates its initiating prompt once',()=>{
 const reply={...receipt('queued','t2','prompt',100),mode:'QUEUE' as const};
 const result=chatTimeline([m('t2','u2','user','prompt\nMaterial manifest: demo.txt'),source],[],[reply],source,true);
 expect(result.map(x=>x.id)).toEqual(['u2','a2']);expect(result[0].receipt?.id).toBe('queued');
});
