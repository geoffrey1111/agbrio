import * as mediaModule from "../codex/messageMedia";
import { act,cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WatchChat } from "./WatchChat";
import type { WatchChatApi, WatchChatState, WatchReply } from "./watchChatApi";
const original={sequence:17,threadId:"exact-original",label:"原对话",cwd:"D:\\project-a",snapshot:{state:"RESULT_READY",turnId:"turn-a",itemId:"item-a",text:"完整原文"},observedAt:1};
const state:WatchChatState={watch:{...original,enabled:true,generation:2,checkedAt:1,errorCode:null},host:"电脑A",ownedTurnId:null,externalBusy:false,replies:[],requests:[],goal:null};
const response=(id:string,status:WatchReply["status"]="SENT"):WatchReply=>({id,threadId:original.threadId,sourceSequence:17,expectedTurnId:"turn-a",mode:"SEND",text:"第一条回复",status,turnId:"turn-b",errorCode:null,createdAt:1,options:{attachments:[]}});
beforeEach(()=>sessionStorage.clear());afterEach(cleanup);
it('opens one chronological native transcript and prepends older pages without duplicating receipts',async()=>{
 const message=(turnId:string,id:string,role:'user'|'assistant',text:string)=>({turnId,id,role,text});
 const command=vi.fn(async(input)=>input.cursor?{messages:[message('turn-old','u-old','user','更早的问题'),message('turn-old','a-old','assistant','更早的回答')],nextCursor:null}:{messages:[message('turn-a','u-a','user','当前的问题'),message('turn-a','item-a','assistant','完整原文'),message('turn-before','u-before','user','之前的问题'),message('turn-before','a-before','assistant','之前的回答')],nextCursor:'older'});
 const receipt={...response('r'),turnId:'turn-a',text:'当前的问题'};
 const view=render(<WatchChat original={original} api={{state:vi.fn(async()=>({...state,replies:[receipt]})),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 await screen.findByRole('button',{name:'加载更早消息'});
 const rows=()=>[...view.container.querySelectorAll('.v4-chat-reader>.v4-chat-message')].map(e=>e.textContent);
 expect(rows()).toEqual([expect.stringContaining('之前的问题'),expect.stringContaining('之前的回答'),expect.stringContaining('当前的问题'),expect.stringContaining('完整原文')]);
 expect(screen.getAllByText('当前的问题')).toHaveLength(1);
 fireEvent.click(screen.getByRole('button',{name:'加载更早消息'}));await screen.findByText('更早的回答');
 expect(rows()[0]).toContain('更早的问题');expect(rows().at(-1)).toContain('完整原文');
 expect(command.mock.calls.every(([input])=>input.action==='HISTORY'&&input.threadId==='exact-original')).toBe(true);
});
it("sends only to the notified thread and focuses its reader heading",async()=>{const command=vi.fn(async(input)=>response(input.id));const api:WatchChatApi={state:vi.fn(async()=>state),command:command as WatchChatApi["command"]};render(<WatchChat original={original} api={api} onBack={()=>{}}/>);await waitFor(()=>expect(screen.getByRole("heading",{name:"原对话"})).toHaveFocus());fireEvent.change(screen.getByLabelText("回复这个 Codex 对话"),{target:{value:"第一条回复"}});await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));await waitFor(()=>expect(command).toHaveBeenCalled());expect(command.mock.calls.find(([x])=>x.action==="SEND")?.[0]).toMatchObject({action:"SEND",threadId:"exact-original",sourceSequence:17,expectedTurnId:"turn-a",generation:2,text:"第一条回复"});});
it("lost HTTP acknowledgement blocks another UUID, survives reopen and reconciles exact receipt",async()=>{let id="";const command=vi.fn(async(input)=>{if(input.action==="SEND"){id=input.id as string;throw Error("network lost");}if(input.action==="RECEIPT")return response(id);throw Error("unexpected");});const api:WatchChatApi={state:vi.fn(async()=>({...state,replies:id?[response(id)]:[]})),command:command as WatchChatApi["command"]};const view=render(<WatchChat original={original} api={api} onBack={()=>{}}/>);fireEvent.change(screen.getByLabelText("回复这个 Codex 对话"),{target:{value:"第一条回复"}});await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));await waitFor(()=>expect(screen.getByRole("button",{name:"检查发送记录"})).toBeEnabled());expect(screen.getByRole("button",{name:"发送回复"})).toBeDisabled();view.unmount();render(<WatchChat original={original} api={api} onBack={()=>{}}/>);await screen.findByRole("button",{name:"检查发送记录"});expect(screen.getByRole("button",{name:"发送回复"})).toBeDisabled();fireEvent.click(screen.getByRole("button",{name:"检查发送记录"}));await waitFor(()=>expect(screen.getByLabelText("回复这个 Codex 对话")).toHaveValue(""));expect(command.mock.calls.filter(([x])=>x.action==="SEND")).toHaveLength(1);expect(command.mock.calls.find(([x])=>x.action==="RECEIPT")?.[0].id).toBe(id);});
it("preserves text entered while the submitted draft awaits acknowledgement",async()=>{let finish:(v:WatchReply)=>void=()=>{};let id="";const command=vi.fn(input=>{id=input.id as string;return new Promise<WatchReply>(r=>{finish=r;});});const api:WatchChatApi={state:vi.fn(async()=>state),command:command as WatchChatApi["command"]};render(<WatchChat original={original} api={api} onBack={()=>{}}/>);const input=screen.getByLabelText("回复这个 Codex 对话");fireEvent.change(input,{target:{value:"第一条回复"}});await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));await waitFor(()=>expect(command).toHaveBeenCalled());fireEvent.change(input,{target:{value:"正在准备下一条"}});finish(response(id));await waitFor(()=>expect(input).toHaveValue("正在准备下一条"));});
it("queues busy external work without offering unproven stop or steer",async()=>{const api:WatchChatApi={state:vi.fn(async()=>({...state,externalBusy:true})),command:vi.fn()};render(<WatchChat original={original} api={api} onBack={()=>{}} backLabel="监听对话"/>);await screen.findByRole("button",{name:"排队，完成后发送"});expect(screen.queryByRole("button",{name:"停止本次执行"})).toBeNull();expect(screen.queryByRole("button",{name:"调整当前任务"})).toBeNull();expect(screen.getByRole("button",{name:"‹ 监听对话"})).toBeVisible();});
it('shows shared follow-up controls without claiming a Desktop turn and steers the exact current turn',async()=>{
 const command=vi.fn(async(input)=>input.action==='HISTORY'?{messages:[],nextCursor:null}:{...response(input.id),mode:'STEER',turnId:'desktop-turn'});
 const api:WatchChatApi={state:vi.fn(async()=>({...state,externalBusy:true,controllableTurnId:'desktop-turn',watch:{...state.watch,snapshot:{...state.watch.snapshot,state:'RUNNING',turnId:'desktop-turn'}}})),command:command as WatchChatApi['command']};render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 await screen.findByRole('button',{name:'停止本次执行'});fireEvent.click(screen.getByRole('button',{name:'调整当前任务'}));fireEvent.change(screen.getByLabelText('回复这个 Codex 对话'),{target:{value:'Keep the current task bounded.'}});
 const buttons=screen.getAllByRole('button',{name:'调整当前任务'});fireEvent.click(buttons.at(-1)!);
 await waitFor(()=>expect(command.mock.calls.find(([c])=>c.action==='SEND')?.[0]).toMatchObject({mode:'STEER',expectedTurnId:'desktop-turn',threadId:'exact-original'}));
});

it("pins newer reply media to its own turn and item rather than the notification",async()=>{
 const media=vi.spyOn(mediaModule,"messageMedia").mockImplementation(()=>Object.assign(vi.fn(async()=>({filename:"result.png",mime:"image/png",data:""})),{scopeKey:"fixture"}));
 try{
  const next={...state,watch:{...state.watch,snapshot:{state:"RESULT_READY",turnId:"turn-new",itemId:"item-new",text:"![新结果](result.png)"}}};
  render(<WatchChat original={original} api={{state:vi.fn(async()=>next),command:vi.fn()}} onBack={()=>{}}/>);
  await screen.findByText("Codex · 最新公开回复");
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
 let finish:(r:WatchReply)=>void=()=>{};let id="";
 const command=vi.fn(async(input)=>{if(input.action==="HISTORY")return {messages:[],nextCursor:null};if(input.action==="SEND"){id=input.id as string;return new Promise<WatchReply>(resolve=>{finish=resolve;});}if(input.action==="RECEIPT")return response(id);throw Error("unexpected command");});
 const api:WatchChatApi={state:vi.fn(async()=>state),command:command as WatchChatApi["command"]};
 const view=render(<WatchChat original={original} api={api} onBack={()=>{}}/>);fireEvent.change(screen.getByLabelText("回复这个 Codex 对话"),{target:{value:"第一条回复"}});
 await waitFor(()=>expect(screen.getByRole("button",{name:"发送回复"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"发送回复"}));view.unmount();finish(response(id));
 render(<WatchChat original={original} api={api} onBack={()=>{}}/>);
 await screen.findByRole('button',{name:'检查发送记录'});expect(screen.getByLabelText("回复这个 Codex 对话")).toHaveValue("");
 expect(screen.getByRole("button",{name:"发送回复"})).toBeDisabled();
 fireEvent.click(screen.getByRole('button',{name:'检查发送记录'}));await waitFor(()=>expect(sessionStorage.getItem('aiwr-watch-draft:exact-original:pending')).toBeNull());
 expect(command.mock.calls.filter(([v])=>v.action==="SEND")).toHaveLength(1);
});
it("a known unsent attempt restores its draft without losing newer input",async()=>{
 const command=vi.fn(async(input)=>input.action==="HISTORY"?{messages:[],nextCursor:null}:{...response(input.id),status:'FAILED'});
 render(<WatchChat original={original} api={{state:vi.fn(async()=>state),command:command as WatchChatApi['command']}} onBack={()=>{}}/>);
 fireEvent.change(screen.getByLabelText('回复这个 Codex 对话'),{target:{value:'第一条回复'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'发送回复'})).toBeEnabled());fireEvent.click(screen.getByRole('button',{name:'发送回复'}));
 await screen.findByText('已确认这次没有发送。草稿保留，可检查后重新发送。');expect(screen.getByLabelText('回复这个 Codex 对话')).toHaveValue('第一条回复');
 expect(sessionStorage.getItem('aiwr-watch-draft:exact-original:pending')).toBeNull();
});
