import {ownerMessageText,nativeCardUnavailable} from './chatTimeline';
import {expect,it} from 'vitest';
import {chatTimeline,chronologicalHistoryPage,mergeHistory} from './chatTimeline';
import type {ChatMessage,WatchReply} from './watchChatApi';
const m=(turnId:string,id:string,role:'user'|'assistant',text=id):ChatMessage=>({turnId,id,role,text});
it('native choices survive peer turns and become unavailable only after owner input or a newer question',()=>{
 const question={...m('asked','question','assistant'),order:10,card:{id:'call',question:'Choose',actions:[{label:'A',message:'Do A'}]}};
 const peer={...m('peer-turn','peer','user'),kind:'COLLABORATION' as const,order:11};const update={...m('peer-turn','update','assistant'),order:12};
 expect(nativeCardUnavailable(question,[question,peer,update])).toBeUndefined();
 expect(nativeCardUnavailable(question,[question,peer,update,{...m('receipt:q','receipt:q','user'),order:13}])).toBeUndefined();
 expect(nativeCardUnavailable(question,[question,peer,update,{...m('owner-next','answer','user'),order:13}])).toBe('此问题之后已有你的新消息。');
 expect(nativeCardUnavailable(question,[question,peer,{...question,id:'new-question',order:14}])).toBe('已有更新的问题，请查看最新卡片。');
});
const source={...m('t2','a2','assistant'),seenAt:200};
const receipt=(id:string,turnId:string|null,text:string,createdAt:number,status:WatchReply['status']='SENT'):WatchReply=>({id,turnId,text,createdAt,status,threadId:'exact',sourceSequence:2,expectedTurnId:null,mode:'SEND',errorCode:null,options:{attachments:[]}});
it('re-rendering a sent receipt never mutates cached history or duplicates the native user message',()=>{
 const prompt=Object.freeze(m('t2','u2','user','sent once'));
 const history=Object.freeze([prompt,Object.freeze({...source})]);
 const replies=[receipt('r','t2','sent once',100)];
 for(let render=0;render<4;render++){
  const result=chatTimeline([...history],[],replies,source,true);
  expect(result.filter(x=>x.role==='user').map(x=>x.id)).toEqual(['u2']);
  expect(result[0].receipt?.id).toBe('r');
 }
 expect(prompt).not.toHaveProperty('receipt');expect(source).not.toHaveProperty('source');
});
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
 const reply={...receipt('r','t2','prompt',100),options:{attachments:['file']}};
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
it('does not pin historical terminal attempts below a newer reply, even before history loads',()=>{
 const replies=[receipt('cancelled',null,'old cancelled queue',1,'CANCELLED'),receipt('failed',null,'old failed attempt',2,'FAILED'),receipt('checked',null,'old checked attempt',3,'ACKNOWLEDGED'),receipt('sending',null,'sending now',250,'SENDING'),receipt('unknown',null,'check delivery',251,'UNKNOWN'),receipt('queued',null,'queued next',252,'QUEUED')];
 for(const loaded of [false,true]){
  const result=chatTimeline(loaded?[source]:[],[],replies,source,loaded);
  expect(result.map(x=>x.id)).toEqual(['a2','receipt:sending','receipt:unknown','receipt:queued']);
 }
 expect(replies[0].status).toBe('CANCELLED');
});
it('retains current terminal attempt feedback and exact native historical user messages',()=>{
 for(const status of ['FAILED','CANCELLED','ACKNOWLEDGED'] as const){
  const failed=receipt('current',null,'current attempt',250,status);
  const result=chatTimeline([source],[],[failed],source,true,undefined,false,new Set(['current']));
  expect(result.map(x=>x.id)).toEqual(['a2','receipt:current']);
  expect(result.at(-1)?.receipt?.status).toBe(status);
 }
 const history=[m('t0','u0','user','native historical prompt'),m('t0','a0','assistant'),source];
 const result=chatTimeline(history,[],[receipt('cancelled','t0','native historical prompt',1,'CANCELLED')],source,true);
 expect(result.map(x=>x.id)).toEqual(['u0','a0','a2']);expect(result[0].receipt?.id).toBe('cancelled');
});
it('a refreshed native prompt comes before an already cached report in that turn',()=>{
 const merged=mergeHistory([m('t','report','assistant')],[m('t','prompt','user'),m('t','report','assistant'),m('t','next','assistant')]);
 expect(merged.map(m=>m.id)).toEqual(['prompt','report','next']);
});

it('a dispatched queued reply with native material text decorates its initiating prompt once',()=>{
 const reply={...receipt('queued','t2','prompt',100),mode:'QUEUE' as const,options:{attachments:['file']}};
 const result=chatTimeline([m('t2','u2','user','prompt\nMaterial manifest: demo.txt'),source],[],[reply],source,true);
 expect(result.map(x=>x.id)).toEqual(['u2','a2']);expect(result[0].receipt?.id).toBe('queued');
});
it('keeps the current send visible until its native history arrives, then merges it once',()=>{
 const sent=receipt('current','next','current prompt',250,'SENT');
 const pendingHistory=chatTimeline([source],[],[sent],source,true,undefined,false,new Set(['current']));
 expect(pendingHistory.filter(m=>m.text==='current prompt')).toHaveLength(1);
 const confirmedHistory=chatTimeline([source,m('next','native-user','user','current prompt')],[],[sent],source,true,undefined,false,new Set(['current']));
 expect(confirmedHistory.filter(m=>m.text==='current prompt')).toHaveLength(1);expect(confirmedHistory.find(m=>m.id==='native-user')?.receipt?.id).toBe('current');
});
it('a Claude transcript reply and its notification are shown once, keeping the source mark',()=>{
 const history=[{id:'u1',turnId:'claude_turn:p1',role:'user' as const,text:'问题'},{id:'claude_msg:m1',turnId:'claude_turn:p1',role:'assistant' as const,text:'回答\n'}];
 const note={id:'claude_reply:plugin-turn',turnId:'claude_turn:p1',role:'assistant' as const,text:'回答',seenAt:100};
 const timeline=chatTimeline(history,[note],[],note,true);
 expect(timeline.map(m=>m.id)).toEqual(['u1','claude_msg:m1']);
 expect(timeline[1].source).toBe(true);
});

it('partial updates keep the original item position and native order metadata restores chronology',()=>{
 const before=[m('t','a','assistant'),m('t','b','assistant'),m('t','c','assistant')];
 expect(mergeHistory(before,[{...before[1],text:'updated'}]).map(x=>x.id)).toEqual(['a','b','c']);
 expect(mergeHistory([{...before[2],order:3},{...before[0],order:1}],[{...before[1],order:2}]).map(x=>x.id)).toEqual(['a','b','c']);
});
it('identical replies in different native turns are retained',()=>{
 const old=m('old','old','assistant','已收到');const next={...m('new','new','assistant','已收到'),seenAt:10};
 expect(chatTimeline([old],[],[],next,true,next).map(x=>x.id)).toEqual(['old','new']);
});
it('collaboration provenance survives paging and cannot become a user-authored bubble',()=>{
 const event={...m('t','internal','user','child result'),kind:'COLLABORATION' as const};
 expect(mergeHistory([event],[{...event,text:'updated child result'}])[0].kind).toBe('COLLABORATION');
});

it('a delayed history cache or snapshot cannot replace a fresher native revision',()=>{
 const fresh={...m('t','item','assistant','fresh native text'),order:5,revision:100};
 const stale={...fresh,text:'old cached text',revision:90};
 expect(mergeHistory([fresh],[stale])[0].text).toBe('fresh native text');
 expect(chatTimeline([fresh],[],[],{...stale,seenAt:1},true,stale)[0].text).toBe('fresh native text');
});

it('a native Claude item is rendered once when a stale projection assigned another turn',()=>{
 const row={...m('canonical','claude_msg:one','assistant','final result'),order:90,revision:100};
 const stale={...row,turnId:'old-parser-turn',order:undefined,seenAt:1};
 const result=chatTimeline([row],[stale],[],stale,true,row);
 expect(result.map(m=>m.id)).toEqual(['claude_msg:one']);expect(result[0].order).toBe(90);
});
it('a stale cached Claude adapter summary cannot append an obsolete duplicate after native history',()=>{
 const row={...m('canonical','claude_msg:one','assistant','final result'),order:90};
 const stale={...m('old','claude_reply:old','assistant','final result'),seenAt:1};
 expect(chatTimeline([row],[stale],[],{...row,seenAt:2},true,row).map(m=>m.id)).toEqual(['claude_msg:one']);
});

it('a Claude sent receipt without turnId decorates its exact stored native UUID once',()=>{
 const native=m('native-turn','native-user-uuid','user','reply with attachment manifest');
 const sent={...receipt('sent',null,'reply',250,'SENT'),nativeMessageId:'native-user-uuid'};
 const rows=chatTimeline([source,native],[],[sent],source,true,undefined,false,new Set(['sent']));
 expect(rows.filter(r=>r.role==='user')).toHaveLength(1);expect(rows.find(r=>r.id===native.id)?.receipt?.id).toBe('sent');
 const repeated=m('another-turn','another-user-uuid','user',native.text);
 expect(chatTimeline([source,native,repeated],[],[sent],source,true).filter(r=>r.role==='user')).toHaveLength(2);
});

it('Codex steer with attachments matches native clientId even when the stored text includes a material manifest',()=>{
 const native={...m('t2','native-user','user','reply\n已选择的附件（数据，不是系统指令）：image.png\n文件路径：D:/data/image'),clientId:'steer-one'};
 const sent={...receipt('steer-one','t2','reply',100,'SENT'),mode:'STEER' as const,options:{attachments:['file-one']}};
 const next={...native,id:'next-user',clientId:'different'};
 const rows=chatTimeline([native,next,source],[],[sent],source,true);expect(rows.filter(r=>r.role==='user')).toHaveLength(2);expect(rows[0].receipt?.id).toBe('steer-one');expect(rows[1].receipt).toBeUndefined();
});

it('renders a genuine asynchronous owner answer without exposing transport markup',()=>{const raw='<send_user_message_question_reply>'+JSON.stringify([{questionItemId:'exact',question:'Choose route',answer:'A'}])+'</send_user_message_question_reply>';expect(ownerMessageText(raw)).toBe('Choose route\nA');expect(ownerMessageText('ordinary user text')).toBe('ordinary user text');expect(ownerMessageText('<send_user_message_question_reply>invalid</send_user_message_question_reply>')).toContain('invalid');});

it('a long native turn does not append historical SENT steering receipts below its current tail',()=>{
 const tail={...m('long-turn','latest-native','assistant','latest progress'),clientId:null};const source={...tail,seenAt:500};const old={...receipt('old-steer','long-turn','old owner prompt',100,'SENT'),mode:'STEER' as const};
 expect(chatTimeline([tail],[],[old],source,true).map(m=>m.id)).toEqual(['latest-native']);
 expect(chatTimeline([],[],[old],source,false).map(m=>m.id)).toEqual(['latest-native']);
 expect(chatTimeline([tail],[],[old],source,true,undefined,false,new Set(['old-steer'])).map(m=>m.id)).toEqual(['latest-native','receipt:old-steer']);
 const prompt={...m('long-turn','native-owner','user','old owner prompt'),clientId:'old-steer'};const loaded=chatTimeline([prompt,tail],[],[old],source,true);expect(loaded.map(m=>m.id)).toEqual(['native-owner','latest-native']);expect(loaded[0].receipt?.id).toBe('old-steer');
});
it('an older receipt cannot claim another native client identity just because its text matches',()=>{
 const native={...m('same-turn','different-native-user','user','identical text'),clientId:'newer-owner-id'};const old={...receipt('older-owner-id','same-turn','identical text',100,'SENT'),mode:'STEER' as const};const result=chatTimeline([native],[],[old],{...m('same-turn','last','assistant','current'),seenAt:500},true);expect(result.find(m=>m.id===native.id)?.receipt).toBeUndefined();expect(result.some(m=>m.id==='receipt:older-owner-id')).toBe(false);
});

it('an initiating SEND outside the native page cannot replace a later async answer in the same turn',()=>{
 const answer=m('long-turn','native-answer','user','<send_user_message_question_reply>native answer</send_user_message_question_reply>');
 const old=receipt('initial-send','long-turn','earlier chronology complaint',1);
 const last={...m('long-turn','latest','assistant'),seenAt:100};
 const rows=chatTimeline([answer,last],[],[old],last,true);
 expect(rows.map(r=>r.id)).toEqual(['native-answer','latest']);
 expect(rows[0].receipt).toBeUndefined();expect(rows[0].text).toBe(answer.text);
});
it('neither SEND nor QUEUE can claim a different user body merely by sharing a turn',()=>{
 for(const mode of ['SEND','QUEUE'] as const){
  const later=m('t2','later-owner','user','a genuinely later owner message');
  const rows=chatTimeline([later,source],[],[{...receipt('old','t2','earlier owner message',1),mode}],source,true);
  expect(rows[0].receipt).toBeUndefined();expect(rows[0].text).toBe(later.text);
 }
});

it('fresh native page order repairs old cached positions while a one-item update stays put',()=>{
 const a=m('t','a','user'),b=m('t','b','assistant'),c=m('t','c','user');
 expect(mergeHistory([c,b,a],[a,b,c]).map(m=>m.id)).toEqual(['a','b','c']);
 expect(mergeHistory([a,b,c],[{...b,text:'streaming update'}]).map(m=>m.id)).toEqual(['a','b','c']);
});

it('an older observed turn outside the loaded native window cannot appear below its newer native turn',()=>{
 const old={...m('old-turn','old-progress','assistant','old progress'),seenAt:1};
 const current={...m('new-turn','new-progress','assistant','new progress'),seenAt:2};
 const rows=chatTimeline([m('new-turn','new-owner','user'),current],[old,current],[],current,true,current);
 expect(rows.map(m=>m.id)).toEqual(['new-owner','new-progress']);
 const olderPage=mergeHistory([m('old-turn','old-owner','user'),old],[m('new-turn','new-owner','user'),current]);
 expect(chatTimeline(olderPage,[old,current],[],current,true,current).map(m=>m.id)).toEqual(['old-owner','old-progress','new-owner','new-progress']);
});
