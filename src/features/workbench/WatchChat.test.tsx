import * as mediaModule from "../codex/messageMedia";
import { act,cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WatchChat,chatError } from "./WatchChat";
import type { ChatMessage, WatchChatApi, WatchChatState, WatchReply } from "./watchChatApi";
const original={sequence:17,threadId:"exact-original",label:"原对话",cwd:"D:\\project-a",snapshot:{state:"RESULT_READY",turnId:"turn-a",itemId:"item-a",text:"完整原文"},observedAt:1};
const state:WatchChatState={watch:{...original,enabled:true,generation:2,checkedAt:1,errorCode:null},host:"电脑A",ownedTurnId:null,externalBusy:false,replies:[],requests:[],goal:null};
const response=(id:string,status:WatchReply["status"]="SENT"):WatchReply=>({id,threadId:original.threadId,sourceSequence:17,expectedTurnId:"turn-a",mode:"SEND",text:"第一条回复",status,turnId:"turn-b",errorCode:null,createdAt:1,options:{attachments:[]}});
it('shows child execution and collapses coordination without exposing it as an owner bubble',async()=>{
 const native={...original,threadId:'claude_code:10000000-0000-4000-8000-000000000001'};
 const taskTree={nodes:[{sessionId:'child',parentSessionId:'root',title:'执行子对话',depth:1,state:'RUNNING',updatedAt:1},{sessionId:'grandchild',parentSessionId:'child',title:'孙对话',depth:2,state:'TURN_ENDED',updatedAt:1}],running:1,attention:0,unknown:0,limited:false};
 const messages=[{id:'human',turnId:'turn-a',role:'user',text:'真实用户输入',order:1},{id:'coord',turnId:'internal',role:'user',text:'internal payload',kind:'COLLABORATION',order:2},{id:'item-a',turnId:'turn-a',role:'assistant',text:'完整原文',order:3}];
 const command=vi.fn(async()=>({messages,nextCursor:null,coverage:'SESSION_TRANSCRIPT'}));
 const view=render(<WatchChat original={native} api={{state:async()=>({...state,watch:{...state.watch,threadId:native.threadId},taskTree}),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 await screen.findByText('真实用户输入');expect(screen.getAllByText(/子对话执行中/).length).toBeGreaterThan(0);
 const internal=view.container.querySelector('.v4-chat-collaboration')!;expect(internal.tagName).toBe('DETAILS');expect(internal).not.toHaveAttribute('open');expect(internal.closest('[data-role="user"]')).toBeNull();
 fireEvent.click(screen.getByText(/查看任务树/));expect(screen.getByText('执行子对话')).toBeVisible();
 expect(command.mock.calls.every(c=>(c as unknown as [{action:string}])[0]?.action==='HISTORY')).toBe(true);
});
beforeEach(()=>sessionStorage.clear());afterEach(()=>{cleanup();vi.unstubAllGlobals();});
it('does not turn pinch zoom into keyboard height and keeps the draft on dismissal',async()=>{
 const viewport=Object.assign(new EventTarget(),{width:390,height:844,offsetTop:0,offsetLeft:0,scale:1});
 vi.stubGlobal('visualViewport',viewport);vi.stubGlobal('innerWidth',390);
 const api:WatchChatApi={state:async()=>state,command:vi.fn(async()=>({messages:[],nextCursor:null})) as WatchChatApi['command']};
 const view=render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 const input=await screen.findByRole('textbox');act(()=>input.focus());fireEvent.change(input,{target:{value:'保留草稿'}});
 const shell=view.container.querySelector<HTMLElement>('.v4-watch-chat')!;
 act(()=>{viewport.height=440;viewport.dispatchEvent(new Event('resize'));});
 expect(shell.style.getPropertyValue('--aiwr-chat-height')).toBe('440px');expect(shell.dataset.keyboard).toBe('true');
 act(()=>{viewport.scale=1.5;viewport.height=300;viewport.dispatchEvent(new Event('resize'));});
 expect(shell.style.getPropertyValue('--aiwr-chat-height')).toBe('');expect(shell.dataset.keyboard).toBe('false');
 act(()=>{viewport.scale=1;viewport.height=844;viewport.dispatchEvent(new Event('resize'));});
 expect(shell.style.getPropertyValue('--aiwr-chat-height')).toBe('844px');expect(input).toHaveValue('保留草稿');
 expect(shell.dataset.keyboard).toBe('false');
});
it('a quiet Claude sender retains the draft and only queues after an explicit choice',async()=>{
 const native={...original,threadId:'claude_code:10000000-0000-4000-8000-000000000001'};
 const chat={...state,provider:'CLAUDE_CODE' as const,inputReady:false,watch:{...state.watch,threadId:native.threadId}};
 let queued:WatchReply|undefined;
 const command=vi.fn(async(input:Record<string,unknown>)=>{
  if(input.action==='SEND'){queued={...response(input.id as string,'QUEUED'),threadId:native.threadId,text:input.text as string};return queued;}
  return {messages:[],nextCursor:null};
 });
 render(<WatchChat original={native} api={{state:async()=>({...chat,replies:queued?[queued]:[]}),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 expect(screen.queryByText(/回复会先排队/)).toBeNull();
 fireEvent.change(screen.getByRole('textbox'),{target:{value:'Owner approved actual reply'}});
 await screen.findByText('发送连接尚未就绪，可选择排队');
 expect(screen.getByRole('button',{name:'调整当前任务'})).toBeDisabled();expect(command.mock.calls.filter(([c])=>c.action==='SEND')).toHaveLength(0);fireEvent.click(screen.getByRole('button',{name:'排队'}));fireEvent.click(screen.getByRole('button',{name:'排队，完成后发送'}));
 await screen.findByRole('button',{name:'已排队'});expect(screen.getAllByRole('button',{name:'已排队'})).toHaveLength(1);expect(screen.queryByText(/请先处理这条回复/)).toBeNull();
 expect(command.mock.calls.find(([c])=>c.action==='SEND')?.[0]).toMatchObject({threadId:native.threadId,text:'Owner approved actual reply'});
 expect(screen.getByRole('textbox')).toHaveValue('');
});
it('Claude uses the exact original session and inherited settings without acknowledgement or duplicate rows',async()=>{
 const native={...original,threadId:'claude_code:10000000-0000-4000-8000-000000000001'};
 let receipt:WatchReply|undefined;
 const command=vi.fn(async(c)=>{
  if(c.action==='HISTORY')return {messages:[...(receipt?.status==='SENT'?[{id:`receipt:${receipt.id}`,turnId:`receipt:${receipt.id}`,role:'user',text:receipt.text}]:[]),{id:'item-a',turnId:'turn-a',role:'assistant',text:'完整原文'}],nextCursor:null};
  if(c.action==='UPLOAD')return {id:'10000000-0000-4000-8000-000000000099',name:'phone.png',size:3};
  if(c.action==='SEND'){receipt={...response(c.id,'QUEUED'),threadId:native.threadId,text:c.text,turnId:null,options:c.options};return receipt;}
  throw Error('unexpected command');
 });
 const api:WatchChatApi={state:async()=>({...state,provider:'CLAUDE_CODE',inputReady:true,watch:{...state.watch,threadId:native.threadId},replies:receipt?[receipt]:[]}),command:command as WatchChatApi['command']};
 const view=render(<WatchChat original={native} api={api} onBack={()=>{}}/>);
 expect(screen.queryByText(/回复会在原 Claude Code 对话空闲时送达/)).toBeNull();
 expect(screen.getByRole('button',{name:'＋ 附件'})).toBeEnabled();
 fireEvent.change(view.container.querySelector('input[type=file]')!,{target:{files:[new File(['png'],'phone.png',{type:'image/png'})]}});await screen.findByRole('button',{name:'移除 phone.png'});
 fireEvent.change(screen.getByRole('textbox'),{target:{value:'Actual requested text'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'发送回复'})).toBeEnabled());
 fireEvent.click(screen.getByRole('button',{name:'发送回复'}));
 await waitFor(()=>expect(command.mock.calls.find(([c])=>c.action==='SEND')?.[0]).toMatchObject({threadId:native.threadId,text:'Actual requested text',mode:'SEND',options:{model:null,effort:null,attachments:['10000000-0000-4000-8000-000000000099']}}));
 expect(screen.queryByRole('button',{name:'知道了'})).not.toBeInTheDocument();
 await waitFor(()=>expect(screen.getAllByText('Actual requested text')).toHaveLength(1));
 receipt!.status='SENT';view.unmount();render(<WatchChat original={native} api={api} onBack={()=>{}}/>);
 await waitFor(()=>expect(screen.getAllByText('Actual requested text')).toHaveLength(1));
});
it('a normal send needs no acknowledgement and native history stays single across composer edits',async()=>{
 let sent:WatchReply|undefined;
 const command=vi.fn(async(input)=>{
  if(input.action==='HISTORY')return {messages:[...(sent?[{id:'native-prompt',turnId:'turn-b',role:'user',text:sent.text}]:[]),{id:'item-a',turnId:'turn-a',role:'assistant',text:'完整原文'}],nextCursor:null};
  if(input.action==='SEND'){sent=response(input.id);return sent;}
  throw Error('unexpected command');
 });
 const api:WatchChatApi={state:vi.fn(async()=>({...state,replies:sent?[sent]:[],watch:{...state.watch,snapshot:sent?{state:'RUNNING',turnId:'turn-b',itemId:null,text:''}:state.watch.snapshot}})),command:command as WatchChatApi['command']};
 render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 const input=screen.getByLabelText('回复这个 Codex 对话');fireEvent.change(input,{target:{value:'第一条回复'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'发送回复'})).toBeEnabled());fireEvent.click(screen.getByRole('button',{name:'发送回复'}));
 await waitFor(()=>expect(input).toHaveValue(''));await waitFor(()=>expect(screen.getAllByText('第一条回复')).toHaveLength(1));
 expect(screen.queryByRole('button',{name:'知道了'})).toBeNull();expect(screen.queryByText('回复已发送到这个原对话。')).toBeNull();
 fireEvent.change(input,{target:{value:'准备下一条'}});expect(screen.getAllByText('第一条回复')).toHaveLength(1);
 expect(command.mock.calls.filter(([v])=>v.action==='SEND')).toHaveLength(1);
});
it('opens one chronological native transcript and prepends older pages without duplicating receipts',async()=>{
 const message=(turnId:string,id:string,role:'user'|'assistant',text:string)=>({turnId,id,role,text});
 const command=vi.fn(async(input)=>input.cursor?{messages:[message('turn-old','u-old','user','更早的问题'),message('turn-old','a-old','assistant','更早的回答')],nextCursor:null}:{messages:[message('turn-a','u-a','user','当前的问题'),message('turn-a','item-a','assistant','完整原文'),message('turn-before','u-before','user','之前的问题'),message('turn-before','a-before','assistant','之前的回答')],nextCursor:'older'});
 const receipt={...response('r'),turnId:'turn-a',text:'当前的问题'};
 const view=render(<WatchChat original={original} api={{state:vi.fn(async()=>({...state,replies:[receipt]})),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 await screen.findByRole('button',{name:'加载更早消息'});
 const rows=()=>[...view.container.querySelectorAll('.v4-chat-reader>.v4-chat-message-row>.v4-chat-message')].map(e=>e.textContent);
 expect(rows()).toEqual([expect.stringContaining('之前的问题'),expect.stringContaining('之前的回答'),expect.stringContaining('当前的问题'),expect.stringContaining('完整原文')]);
 expect(screen.getAllByText('当前的问题')).toHaveLength(1);
 fireEvent.click(screen.getByRole('button',{name:'加载更早消息'}));await screen.findByText('更早的回答');
 expect(rows()[0]).toContain('更早的问题');expect(rows().at(-1)).toContain('完整原文');
 expect(command.mock.calls.every(([input])=>input.action==='HISTORY'&&input.threadId==='exact-original')).toBe(true);
});
it("sends only to the notified thread and focuses its reader heading",async()=>{const command=vi.fn(async(input)=>response(input.id));const api:WatchChatApi={state:vi.fn(async()=>state),command:command as WatchChatApi["command"]};render(<WatchChat original={original} api={api} onBack={()=>{}}/>);await waitFor(()=>expect(screen.getByRole("heading",{name:"原对话"})).toHaveFocus());fireEvent.change(screen.getByLabelText("回复这个 Codex 对话"),{target:{value:"第一条回复"}});await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));await waitFor(()=>expect(command).toHaveBeenCalled());expect(command.mock.calls.find(([x])=>x.action==="SEND")?.[0]).toMatchObject({action:"SEND",threadId:"exact-original",sourceSequence:17,expectedTurnId:"turn-a",generation:2,text:"第一条回复"});});
it("lost HTTP acknowledgement reconciles the exact receipt automatically and never resends",async()=>{let id="";const command=vi.fn(async(input)=>{if(input.action==="SEND"){id=input.id as string;throw Error("network lost");}if(input.action==="RECEIPT")return response(id);throw Error("unexpected");});const api:WatchChatApi={state:vi.fn(async()=>({...state,replies:id?[response(id)]:[]})),command:command as WatchChatApi["command"]};const view=render(<WatchChat original={original} api={api} onBack={()=>{}}/>);fireEvent.change(screen.getByLabelText("回复这个 Codex 对话"),{target:{value:"第一条回复"}});await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));// The leftover attempt is checked automatically; the Host holds the exact receipt, so it settles without resending.
await waitFor(()=>expect(command.mock.calls.some(([x])=>x.action==="RECEIPT")).toBe(true));view.unmount();render(<WatchChat original={original} api={api} onBack={()=>{}}/>);await waitFor(()=>expect(screen.getByLabelText("回复这个 Codex 对话")).toHaveValue(""));expect(command.mock.calls.filter(([x])=>x.action==="SEND")).toHaveLength(1);expect(command.mock.calls.find(([x])=>x.action==="RECEIPT")?.[0].id).toBe(id);});
it("preserves text entered while the submitted draft awaits acknowledgement",async()=>{let finish:(v:WatchReply)=>void=()=>{};let id="";const command=vi.fn(input=>{id=input.id as string;return new Promise<WatchReply>(r=>{finish=r;});});const api:WatchChatApi={state:vi.fn(async()=>state),command:command as WatchChatApi["command"]};render(<WatchChat original={original} api={api} onBack={()=>{}}/>);const input=screen.getByLabelText("回复这个 Codex 对话");fireEvent.change(input,{target:{value:"第一条回复"}});await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));await waitFor(()=>expect(command).toHaveBeenCalled());fireEvent.change(input,{target:{value:"正在准备下一条"}});finish(response(id));await waitFor(()=>expect(input).toHaveValue("正在准备下一条"));});
it("never silently queues busy work without native steering; queue is an explicit choice",async()=>{
 const command=vi.fn(async c=>c.action==='HISTORY'?{messages:[],nextCursor:null}:response(c.id,'QUEUED'));
 const api:WatchChatApi={state:vi.fn(async()=>({...state,externalBusy:true})),command:command as WatchChatApi['command']};
 render(<WatchChat original={original} api={api} onBack={()=>{}} backLabel="监听对话"/>);
 const input=await screen.findByRole('textbox');fireEvent.change(input,{target:{value:'补充要求'}});
 expect(screen.getByRole('button',{name:'调整当前任务'})).toBeDisabled();expect(screen.getByText('当前暂不支持引导，可选择排队')).toBeVisible();
 expect(screen.getByRole('button',{name:'排队'})).toHaveAttribute('aria-pressed','false');expect(command.mock.calls.filter(([c])=>c.action==='SEND')).toHaveLength(0);
 fireEvent.click(screen.getByRole('button',{name:'排队'}));fireEvent.click(screen.getByRole('button',{name:'排队，完成后发送'}));
 await waitFor(()=>expect(command.mock.calls.find(([c])=>c.action==='SEND')?.[0]).toMatchObject({mode:'QUEUE',queueRequested:true,expectedTurnId:'turn-a'}));
 expect(screen.getByRole('button',{name:'排队'})).toHaveAttribute('aria-pressed','false');expect(screen.queryByRole('button',{name:'停止本次执行'})).toBeNull();expect(screen.getByRole('button',{name:'‹ 监听对话'})).toBeVisible();
});
it('shows shared follow-up controls without claiming a Desktop turn and steers the exact current turn',async()=>{
 const command=vi.fn(async(input)=>input.action==='HISTORY'?{messages:[],nextCursor:null}:{...response(input.id),mode:'STEER',turnId:'desktop-turn'});
 const api:WatchChatApi={state:vi.fn(async()=>({...state,externalBusy:true,controllableTurnId:'desktop-turn',watch:{...state.watch,snapshot:{...state.watch.snapshot,state:'RUNNING',turnId:'desktop-turn'}}})),command:command as WatchChatApi['command']};render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 await screen.findByRole('button',{name:'停止本次执行'});expect(screen.getByRole('button',{name:'引导'})).toHaveAttribute('aria-pressed','true');fireEvent.change(screen.getByLabelText('回复这个 Codex 对话'),{target:{value:'Keep the current task bounded.'}});
 const buttons=screen.getAllByRole('button',{name:'调整当前任务'});fireEvent.click(buttons.at(-1)!);
 await waitFor(()=>expect(command.mock.calls.find(([c])=>c.action==='SEND')?.[0]).toMatchObject({mode:'STEER',expectedTurnId:'desktop-turn',threadId:'exact-original'}));
});

it("pins newer reply media to its own turn and item rather than the notification",async()=>{
 const media=vi.spyOn(mediaModule,"messageMedia").mockImplementation(()=>Object.assign(vi.fn(async()=>({filename:"result.png",mime:"image/png",data:""})),{scopeKey:"fixture"}));
 try{
  const next={...state,watch:{...state.watch,snapshot:{state:"RESULT_READY",turnId:"turn-new",itemId:"item-new",text:"![新结果](result.png)"}}};
  render(<WatchChat original={original} api={{state:vi.fn(async()=>next),command:vi.fn()}} onBack={()=>{}}/>);
  await waitFor(()=>expect(media).toHaveBeenCalledWith({kind:"WATCH",threadId:"exact-original",turnId:"turn-new",itemId:"item-new"}));
  expect(media).toHaveBeenCalledWith({kind:"WATCH",threadId:"exact-original",turnId:"turn-new",itemId:"item-new"});
 }finally{media.mockRestore();}
});


it("browser Back closes only the options sheet and retains the original chat draft",async()=>{
 history.replaceState({},"","/mobile");const api:WatchChatApi={state:vi.fn(async()=>state),command:vi.fn()};
 render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 fireEvent.change(screen.getByLabelText("回复这个 Codex 对话"),{target:{value:"保留尚未发送的输入"}});
 fireEvent.click(screen.getByRole("button",{name:"对话选项"}));await screen.findByRole("dialog",{name:"对话选项"});
 act(()=>history.back());
 await waitFor(()=>expect(screen.queryByRole("dialog",{name:"对话选项"})).toBeNull());
 expect(screen.getByLabelText("回复这个 Codex 对话")).toHaveValue("保留尚未发送的输入");expect(vi.mocked(api.command).mock.calls.filter(([input])=>["SEND","QUEUE","STEER","RESPOND","STOP"].includes(input.action))).toHaveLength(0);
});

it('shows all public progress in native order during running, with a fixed thinking status',async()=>{
 const watch={...state.watch,snapshot:{state:'RUNNING',turnId:'current',itemId:'b',text:'第二阶段汇报'}};
 const current:WatchChatState={...state,watch,externalBusy:true,activity:'THINKING',publicMessages:[{id:'a',turnId:'current',role:'assistant',text:'第一阶段汇报'},{id:'b',turnId:'current',role:'assistant',text:'第二阶段汇报'}]};
 const api:WatchChatApi={state:vi.fn(async()=>current),command:vi.fn(async()=>({messages:[{id:'prompt',turnId:'current',role:'user',text:'当前要求'},{id:'a',turnId:'current',role:'assistant',text:'第一阶段汇报'},{id:'b',turnId:'current',role:'assistant',text:'第二阶段汇报'},{id:'old-prompt',turnId:'old',role:'user',text:'上次要求'},{id:'old-answer',turnId:'old',role:'assistant',text:'上次结果'}],nextCursor:null})) as WatchChatApi['command']};
 const view=render(<WatchChat original={watch} api={api} onBack={()=>{}}/>);
 await screen.findByText('当前要求');await screen.findByText('正在思考');
 const rows=[...view.container.querySelectorAll('.v4-chat-message')].map(row=>row.textContent);
 expect(rows).toEqual([expect.stringContaining('上次要求'),expect.stringContaining('上次结果'),expect.stringContaining('当前要求'),expect.stringContaining('第一阶段汇报'),expect.stringContaining('第二阶段汇报')]);
 expect(view.container.querySelector('.v4-chat-reader')?.contains(screen.getByText('正在思考'))).toBe(false);
});

it("moves one submission into the transcript immediately and rapid submits cannot create another request",async()=>{
 let finish:(r:WatchReply)=>void=()=>{};let id="";
 const command=vi.fn(async(input)=>{if(input.action==="HISTORY")return {messages:[],nextCursor:null};if(input.action==="SEND"){id=input.id as string;return new Promise<WatchReply>(resolve=>{finish=resolve;});}throw Error("unexpected command");});
 const view=render(<WatchChat original={original} api={{state:vi.fn(async()=>state),command:command as WatchChatApi["command"]}} onBack={()=>{}}/>);
 const input=screen.getByLabelText("回复这个 Codex 对话");fireEvent.change(input,{target:{value:"第一条回复"}});
 await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());
 const form=view.container.querySelector('form')!;
 act(()=>{fireEvent.submit(form);fireEvent.submit(form);});
 expect(input).toHaveValue("");expect(screen.getByText("第一条回复")).toBeVisible();
 expect(command.mock.calls.filter(([v])=>v.action==="SEND")).toHaveLength(1);
 expect(JSON.parse(sessionStorage.getItem('aiwr-watch-draft:exact-original')!)).toMatchObject({text:"",files:[]});
 expect(command.mock.calls.find(([v])=>v.action==="SEND")?.[0]).not.toHaveProperty('createdAt');
 finish(response(id));await waitFor(()=>expect(sessionStorage.getItem('aiwr-watch-draft:exact-original:pending')).toBeNull());
 expect(input).toHaveValue("");fireEvent.submit(form);expect(command.mock.calls.filter(([v])=>v.action==="SEND")).toHaveLength(1);
});
it("leaving before acknowledgement keeps the empty composer and exact receipt across reopen",async()=>{
 let confirmed=false;
 let finish:(r:WatchReply)=>void=()=>{};let id="";
 const command=vi.fn(async(input)=>{if(input.action==="HISTORY")return {messages:[],nextCursor:null};if(input.action==="SEND"){id=input.id as string;return new Promise<WatchReply>(resolve=>{finish=resolve;});}if(input.action==="RECEIPT")return response(id,confirmed?"SENT":"UNKNOWN");throw Error("unexpected command");});
 const api:WatchChatApi={state:vi.fn(async()=>state),command:command as WatchChatApi["command"]};
 const view=render(<WatchChat original={original} api={api} onBack={()=>{}}/>);fireEvent.change(screen.getByLabelText("回复这个 Codex 对话"),{target:{value:"第一条回复"}});
 await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));view.unmount();finish(response(id));
 render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'送达尚未确认'}));await screen.findByRole('button',{name:'检查发送记录'});expect(screen.getByLabelText("回复这个 Codex 对话")).toHaveValue("");
 expect(document.querySelector('.v4-chat-composer button[type="submit"]')).toBeDisabled();
 confirmed=true;fireEvent.click(screen.getByRole('button',{name:'检查发送记录'}));await waitFor(()=>expect(sessionStorage.getItem('aiwr-watch-draft:exact-original:pending')).toBeNull());
 expect(command.mock.calls.filter(([v])=>v.action==="SEND")).toHaveLength(1);
});
it('does not display an old cancelled queue after the current result or replay it',async()=>{
 const cancelled={...response('cancelled','CANCELLED'),turnId:null,text:'几天前取消的排队消息'};
 const command=vi.fn(async(input)=>{if(input.action==='HISTORY')return {messages:[{id:'item-a',turnId:'turn-a',role:'assistant',text:'完整原文'}],nextCursor:null};throw Error('unexpected mutation');});
 const view=render(<WatchChat original={original} api={{state:vi.fn(async()=>({...state,replies:[cancelled]})),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 await waitFor(()=>expect(command).toHaveBeenCalled());await screen.findByText('完整原文');
 expect(screen.queryByText(cancelled.text)).toBeNull();expect(screen.queryByText('你 · 已取消排队')).toBeNull();
 expect(view.container.querySelectorAll('[data-message-kind="receipt"]')).toHaveLength(0);
 expect(command.mock.calls.every(([input])=>input.action==='HISTORY')).toBe(true);
});
it('unknown receipt recovery requires the owner click, preserves uncertainty and never sends',async()=>{
 let status:WatchReply['status']='UNKNOWN';
 const unknown={...response('exact-unknown','UNKNOWN'),turnId:null,text:'待核查的旧跟进'};
 const command=vi.fn(async(input)=>{if(input.action==='HISTORY')return {messages:[],nextCursor:null};if(input.action==='ACKNOWLEDGE_UNKNOWN'){status='ACKNOWLEDGED';return {ok:true};}throw Error('unexpected write');});
 const api={state:vi.fn(async()=>({...state,replies:[{...unknown,status,errorCode:status==='UNKNOWN'?'REPLY_ACK_UNOBSERVED':'OWNER_CHECKED_DELIVERY_STILL_UNKNOWN'}]})),command:command as WatchChatApi['command']};
 render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'送达尚未确认'}));await screen.findByRole('dialog',{name:'发送详情'});
 const check=await screen.findByRole('button',{name:'我已检查原对话，继续输入新回复'});
 expect(command.mock.calls.every(([input])=>input.action==='HISTORY')).toBe(true);
 fireEvent.click(check);
 await screen.findByText('已结束这条回复的送达检查，原文仍保留。可以输入新的要求。');
 expect(command.mock.calls.find(([input])=>input.action==='ACKNOWLEDGE_UNKNOWN')?.[0]).toMatchObject({threadId:'exact-original',id:'exact-unknown',confirmed:true});
 expect(status).toBe('ACKNOWLEDGED');expect(screen.getByText('待核查的旧跟进')).toBeVisible();
 expect(command.mock.calls.filter(([input])=>['SEND','QUEUE','STEER'].includes(input.action))).toHaveLength(0);
});
it("a known unsent attempt restores its draft without losing newer input",async()=>{
 let failed:WatchReply|null=null;
 const command=vi.fn(async(input)=>input.action==="HISTORY"?{messages:[],nextCursor:null}:(failed={...response(input.id),status:'FAILED'}));
 render(<WatchChat original={original} api={{state:vi.fn(async()=>({...state,replies:failed?[failed]:[]})),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 fireEvent.change(screen.getByLabelText('回复这个 Codex 对话'),{target:{value:'第一条回复'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'发送回复'})).toBeEnabled());fireEvent.click(screen.getByRole('button',{name:'发送回复'}));
 await screen.findByText('已确认这次没有发送。草稿保留，可检查后重新发送。');expect(screen.getByLabelText('回复这个 Codex 对话')).toHaveValue('第一条回复');
 expect(screen.getByRole('button',{name:'未发送成功'})).toBeVisible();
 expect(sessionStorage.getItem('aiwr-watch-draft:exact-original:pending')).toBeNull();
});
it("an attempt the Host never received is cancelled and its draft comes back for an explicit resend",async()=>{let id="";const command=vi.fn(async(input)=>{if(input.action==="SEND"){id=input.id as string;throw Error("network lost");}if(input.action==="RECEIPT")return null;if(input.action==="ABANDON")return{cancelled:true};throw Error("unexpected");});const api:WatchChatApi={state:vi.fn(async()=>({...state,replies:[]})),command:command as WatchChatApi["command"]};render(<WatchChat original={original} api={api} onBack={()=>{}}/>);fireEvent.change(screen.getByLabelText("回复这个 Codex 对话"),{target:{value:"没收到的回复"}});await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));await waitFor(()=>expect(command.mock.calls.find(([x])=>x.action==="ABANDON")?.[0].id).toBe(id));await waitFor(()=>expect(screen.getByLabelText("回复这个 Codex 对话")).toHaveValue("没收到的回复"));expect(command.mock.calls.filter(([x])=>x.action==="SEND")).toHaveLength(1);});

it('retries a failed initial history read on reconnect and replaces the obsolete directory summary',async()=>{
 const native={threadId:'claude_code:10000000-0000-4000-8000-000000000001',label:'manager',cwd:original.cwd,enabled:true,generation:2,checkedAt:1,errorCode:null,snapshot:{state:'RESULT_READY',turnId:'obsolete',itemId:'claude_reply:obsolete',text:'old directory summary'}};
 const message={id:'claude_msg:latest',turnId:'latest',role:'assistant' as const,text:'latest native conclusion',order:10};
 let attempts=0;
 let reconnected!:(value:WatchChatState)=>void;const api:WatchChatApi={state:()=>new Promise(resolve=>{reconnected=resolve;}),command:vi.fn(async()=>{if(++attempts===1)throw Error('Failed to fetch');return {messages:[message],nextCursor:null}}) as WatchChatApi['command']};
 render(<WatchChat original={native} api={api} onBack={()=>{}}/>);
 await screen.findByRole('alert');act(()=>reconnected({...state,watch:{...native,snapshot:{state:'RESULT_READY',turnId:'latest',itemId:message.id,text:message.text}},publicMessages:[message]}));
 await waitFor(()=>expect(attempts).toBeGreaterThan(1));await waitFor(()=>expect(screen.queryByRole('alert')).toBeNull());
 expect(screen.getAllByText('latest native conclusion')).toHaveLength(1);expect(screen.queryByText('old directory summary')).toBeNull();
});

it('opens an opted-in child result separately without inserting it into the parent transcript',async()=>{
 const native={...original,threadId:'claude_code:10000000-0000-4000-8000-000000000001',label:'manager · 子对话：child',snapshot:{state:'RESULT_READY',turnId:'child-turn',itemId:'child_result:child:result',text:'child result original'}};
 const message={id:'claude_msg:parent',turnId:'parent',role:'assistant' as const,text:'parent conclusion',order:10};
 const api:WatchChatApi={state:async()=>({...state,watch:{...state.watch,threadId:native.threadId,snapshot:{state:'RESULT_READY',turnId:'parent',itemId:message.id,text:message.text}},publicMessages:[message]}),command:vi.fn(async()=>({messages:[message],nextCursor:null})) as WatchChatApi['command']};
 const view=render(<WatchChat original={native} api={api} onBack={()=>{}}/>);await screen.findByText('parent conclusion');
 expect(screen.getByText('child result original').closest('details')).not.toBeNull();expect(view.container.querySelectorAll('.v4-chat-message')).toHaveLength(1);
});

it('the persistent child entry opens the exact child conversation and restores the parent draft',async()=>{
 const native={...original,threadId:'claude_code:10000000-0000-4000-8000-000000000001'};
 const tree={nodes:[{sessionId:'child',parentSessionId:'root',title:'需要处理的子对话',depth:1,state:'FAILED',updatedAt:1}],running:0,attention:1,unknown:0,limited:false};
 const parent={...state,provider:'CLAUDE_CODE' as const,watch:{...state.watch,threadId:native.threadId},taskTree:tree};
 const child={...state,provider:'CLAUDE_CODE' as const,inputReady:true,watch:{...state.watch,threadId:'claude_code:child',label:'需要处理的子对话',generation:-12}};
 const command=vi.fn(async(input:Record<string,unknown>)=>input.action==='CHILD_OPEN'?child:({messages:input.threadId===child.watch.threadId?[{id:'child-message',turnId:'child-turn',role:'assistant',text:'子对话的实际结果',order:1}]:[],nextCursor:null}));
 render(<WatchChat original={native} api={{state:async id=>id===child.watch.threadId?child:parent,command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 await screen.findByRole('button',{name:'查看子对话'});fireEvent.change(screen.getByRole('textbox'),{target:{value:'保留主对话草稿'}});
 fireEvent.click(screen.getByRole('button',{name:'查看子对话'}));expect(await screen.findByRole('dialog')).toBeVisible();
 fireEvent.click(screen.getByRole('button',{name:/需要处理的子对话/}));await screen.findByText('子对话的实际结果');
 expect(command.mock.calls.some(([c])=>c.action==='CHILD_OPEN'&&c.childId==='child'&&c.threadId===native.threadId)).toBe(true);
 expect(screen.queryByRole('dialog')).toBeNull();expect(screen.getByRole('textbox')).toHaveValue('');
 fireEvent.change(screen.getByRole('textbox'),{target:{value:'子对话草稿'}});
 fireEvent.click(screen.getByRole('button',{name:/返回主对话/}));expect(await screen.findByRole('textbox')).toHaveValue('保留主对话草稿');
 expect(command.mock.calls.every(([c])=>['HISTORY','CHILD_OPEN'].includes(String(c.action)))).toBe(true);
});

it('async native Codex questions are interactive in the reader with an exact persistent answer identity',async()=>{
 const card={requestId:'call-original',answerId:'answer-original',revision:1,method:'request_user_input_async',kind:'USER_INPUT',isBlocking:false,responseSent:false,questions:[{id:'["request_user_input_async","call-original",0]',label:'Choose route',required:true,options:[{label:'A',description:'Bounded'},{label:'B',description:'Broad'}]}]};
 const command=vi.fn(async(input:Record<string,unknown>)=>input.action==='ASYNC_RESPOND'?{accepted:true}:{messages:[],nextCursor:null});
 const view=render(<WatchChat original={original} api={{state:async()=>({...state,asyncQuestions:[card]}),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 const radio=await screen.findByRole('radio',{name:'A Bounded'});expect(radio.closest('.v4-chat-reader')).not.toBeNull();fireEvent.click(radio);fireEvent.click(screen.getByRole('button',{name:'发送'}));await waitFor(()=>expect(command.mock.calls.some(([c])=>c.action==='ASYNC_RESPOND'&&c.id==='answer-original'&&c.requestId==='call-original')).toBe(true));expect(view.container.querySelector('.v4-chat-dock .v4-question-card')).toBeNull();
});
it('native Claude turn actions send the original selected message and preserve the existing composer draft',async()=>{
 const native={...original,threadId:'claude_code:10000000-0000-4000-8000-000000000001'};
 const message={id:'claude_msg:wrap',turnId:'turn-a',role:'assistant' as const,text:'阶段总结',order:1,card:{id:'native-wrap',question:'下一步？',actions:[{label:'选择 A',message:'原生选项 A 的完整消息'}]}};
 const command=vi.fn(async(input:Record<string,unknown>)=>input.action==='SEND'?{...response(String(input.id)),threadId:native.threadId,text:String(input.text)}:{messages:[message],nextCursor:null});
 render(<WatchChat original={native} api={{state:async()=>({...state,provider:'CLAUDE_CODE',watch:{...state.watch,threadId:native.threadId},publicMessages:[message]}),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 await screen.findByText('阶段总结');fireEvent.change(screen.getByRole('textbox'),{target:{value:'原有草稿'}});fireEvent.click(screen.getByRole('radio',{name:'选择 A'}));fireEvent.click(screen.getByRole('button',{name:'发送选择'}));await waitFor(()=>expect(command.mock.calls.some(([c])=>c.action==='SEND'&&c.text==='原生选项 A 的完整消息')).toBe(true));expect(screen.getByRole('textbox')).toHaveValue('原有草稿');
});
it('keeps an unanswered native card interactive after a peer turn and explains a later owner answer',async()=>{
 const native={...original,threadId:'claude_code:10000000-0000-4000-8000-000000000001'};
 const question={id:'claude_msg:question',turnId:'asked',role:'assistant' as const,text:'需要决策',order:1,card:{id:'card-call',question:'怎么处理？',actions:[{label:'方案 A',message:'执行方案 A'}]}};
 const peer={id:'peer',turnId:'peer-turn',role:'user' as const,text:'子对话回报',kind:'COLLABORATION' as const,order:2};
 const update={id:'update',turnId:'peer-turn',role:'assistant' as const,text:'已收到进展',order:3};
 let messages:ChatMessage[]=[question,peer,update];const snapshot={state:'RESULT_READY',turnId:'peer-turn',itemId:'update',text:'已收到进展'};
 const current=()=>({...state,provider:'CLAUDE_CODE' as const,watch:{...state.watch,threadId:native.threadId,snapshot},publicMessages:messages});
 const command=vi.fn(async(input:Record<string,unknown>)=>input.action==='SEND'?{...response(String(input.id)),threadId:native.threadId,text:String(input.text)}:{messages,nextCursor:null});
 const view=render(<WatchChat original={native} api={{state:async()=>current(),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 const radio=await screen.findByRole('radio',{name:'方案 A'});expect(radio).toBeEnabled();fireEvent.click(radio);fireEvent.click(screen.getByRole('button',{name:'发送选择'}));
 await waitFor(()=>expect(command.mock.calls.some(([c])=>c.action==='SEND'&&c.expectedTurnId==='peer-turn'&&c.text==='执行方案 A')).toBe(true));view.unmount();
 messages=[question,peer,update,{id:'owner',turnId:'owner-next',role:'user',text:'选择已在原对话处理',order:4}];
 render(<WatchChat original={native} api={{state:async()=>current(),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 await screen.findByText('此问题之后已有你的新消息。');expect(screen.getByRole('radio',{name:'方案 A'})).toBeDisabled();expect(screen.queryByRole('button',{name:'发送选择'})).toBeNull();
});

it('resolved historical async questions leave no trailing confirmation prompts',async()=>{
 const card:NonNullable<WatchChatState['asyncQuestions']>[number]={requestId:'old-question',answerId:'old-answer',revision:1,method:'request_user_input_async',kind:'USER_INPUT',isBlocking:false,responseSent:true,questions:[{id:'q',label:'Already answered',options:[{label:'A',description:''}]}]};
 const api:WatchChatApi={state:async()=>({...state,asyncQuestions:[card,{...card,requestId:'older-question'}]}),command:vi.fn(async()=>({messages:[],nextCursor:null})) as WatchChatApi['command']};
 const view=render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 await screen.findByText('完整原文');await waitFor(()=>expect(view.container.querySelectorAll('.v4-question-receipt,.v4-question-card')).toHaveLength(0));
});
it('a later exact SENT observation clears a previously uncertain attempt without another send or recovery panel',async()=>{
 const attempt={id:'late-proof',text:'第一条回复',options:{attachments:[]},files:[],composerCleared:true};
 sessionStorage.setItem('aiwr-watch-draft:exact-original:pending',JSON.stringify(attempt));
 let current={...state,replies:[response(attempt.id,'UNKNOWN')]};let notify=()=>{};
 const command=vi.fn(async(c)=>c.action==='RECEIPT'?response(attempt.id,'UNKNOWN'):{messages:[],nextCursor:null});
 const api:WatchChatApi={state:async()=>current,cachedState:()=>current,subscribe:(_id,fn)=>{notify=fn;return()=>{};},command:command as WatchChatApi['command']};
 const view=render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'送达尚未确认'}));await screen.findByRole('button',{name:'检查发送记录'});
 expect(view.container.querySelector('.v4-chat-message-row .v4-delivery-trigger')).not.toBeNull();
 expect(screen.queryByText('上一次回复仍需确认，草稿已保留。')).toBeNull();
 current={...current,replies:[response(attempt.id,'SENT')]};act(()=>notify());
 await waitFor(()=>expect(sessionStorage.getItem('aiwr-watch-draft:exact-original:pending')).toBeNull());
 expect(screen.queryByRole('button',{name:'检查发送记录'})).toBeNull();expect(view.container.querySelectorAll('.v4-question-receipt')).toHaveLength(0);
 expect(command.mock.calls.every(([c])=>['HISTORY','RECEIPT'].includes(c.action))).toBe(true);
});
it('a running Codex turn refreshes its native public tail when the native item changes',async()=>{
 let snap={state:'RUNNING',turnId:'turn-a',itemId:'one',text:'first progress'};let notify=()=>{};
 const current=()=>({...state,watch:{...state.watch,snapshot:snap}});
 const command=vi.fn(async()=>({messages:[{id:'native-user',turnId:'turn-a',role:'user',text:'owner request'},{id:snap.itemId,turnId:'turn-a',role:'assistant',text:snap.text}],nextCursor:null}));
 const api:WatchChatApi={state:async()=>current(),cachedState:current,subscribe:(_id,fn)=>{notify=fn;return()=>{};},command:command as WatchChatApi['command']};
 render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 await screen.findByText('owner request');const calls=command.mock.calls.length;
 snap={...snap,itemId:'two',text:'second progress'};act(()=>notify());
 await waitFor(()=>expect(command.mock.calls.length).toBeGreaterThan(calls));await screen.findByText('second progress');
});

it('does not describe a desktop provider disconnect or read timeout as an unreachable computer',()=>{
 expect(chatError(Error('Codex backend disconnected'))).toContain('Codex 读取连接已断开');expect(chatError(Error('ROUTER_READ_TIMEOUT'))).toContain('读取对话超时');expect(chatError(Error('ROUTER_READ_TIMEOUT'))).not.toContain('连不上');
});
it('explicit retry bypasses cached reads, preserves the draft and clears only recovered read errors',async()=>{
 const fresh={...state,watch:{...state.watch,snapshot:{...state.watch.snapshot,state:'RUNNING'}}};
 const api:WatchChatApi={state:vi.fn().mockResolvedValue(fresh),command:vi.fn().mockRejectedValue(Error('Codex backend disconnected')),refreshState:vi.fn().mockResolvedValue(fresh),refreshHistory:vi.fn().mockResolvedValue({messages:[{id:'recovered',turnId:'turn-a',role:'assistant',text:'fresh recovered history'}],nextCursor:null})};
 render(<WatchChat original={original} api={api} onBack={vi.fn()}/>);const warning=await screen.findByRole('alert');expect(warning).toHaveTextContent('Codex 读取连接已断开');expect(screen.getAllByText('状态待确认').length).toBeGreaterThan(0);
 const input=screen.getByRole('textbox');fireEvent.change(input,{target:{value:'preserved draft'}});fireEvent.click(screen.getByRole('button',{name:'检查状态'}));await screen.findByText('fresh recovered history');await waitFor(()=>expect(screen.queryByRole('alert')).toBeNull());expect(api.refreshState).toHaveBeenCalledOnce();expect(api.refreshHistory).toHaveBeenCalledOnce();expect(input).toHaveValue('preserved draft');expect((api.command as any).mock.calls.every((c:any)=>c[0].action==='HISTORY')).toBe(true);
});
