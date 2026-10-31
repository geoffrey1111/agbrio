import {cleanup,fireEvent,render,screen,waitFor,within} from "@testing-library/react";
import {afterEach,beforeEach,expect,it,vi} from "vitest";
import {UnifiedWorkbench,type UnifiedWorkbenchProps} from "./UnifiedWorkbench";
import type {CodexWatch,NotificationApi} from "./CodexNotifications";
import type {WatchChatApi,WatchChatState} from "./watchChatApi";

const realMatchMedia=window.matchMedia;
function viewport(query:(q:string)=>boolean){
 Object.defineProperty(window,"matchMedia",{configurable:true,writable:true,value:(q:string)=>({matches:query(q),media:q,onchange:null,addEventListener:()=>undefined,removeEventListener:()=>undefined,addListener:()=>undefined,removeListener:()=>undefined,dispatchEvent:()=>false})});
}
beforeEach(()=>viewport(q=>q==="(min-width:1024px)"));
afterEach(()=>{cleanup();localStorage.clear();Object.defineProperty(window,"matchMedia",{configurable:true,writable:true,value:realMatchMedia});});

const pi:CodexWatch={threadId:"pi:10000000-0000-4000-8000-000000000011",label:"界面重设计",cwd:"D:/fixture/card",enabled:true,generation:1,checkedAt:Date.now(),errorCode:null,lastMessageAt:Date.now(),snapshot:{state:"RUNNING",turnId:"t",itemId:"i",text:"正在盘点页面"}};
const state:WatchChatState={watch:pi,provider:"PI",inputReady:true,host:"工作电脑",ownedTurnId:null,externalBusy:true,replies:[],requests:[],goal:null,activity:"EXECUTING",publicMessages:[],
 piTask:{taskId:"task",executionEpoch:"e",loaded:true,canRestore:false,state:"RUNNING",settings:{provider:"anthropic",model:"claude-opus-5-5",thinking:"high"},selectionSource:"EXPLICIT",parent:{provider:"CLAUDE_CODE",externalId:"10000000-0000-4000-8000-000000000001",cwd:"D:/fixture/card"}}};
const chat:WatchChatApi={state:async()=>state,command:(async(input:{action:string})=>{if(input.action==="HISTORY")return{messages:[],nextCursor:null};throw Error("FIXTURE_NO_WRITES");}) as WatchChatApi["command"]};
const watchApi:NotificationApi={cachedWatches:()=>[pi],watches:async()=>[pi],threads:async()=>({complete:true,threads:[]}) as never,connect:async()=>undefined,enable:async()=>undefined,pause:async()=>undefined,feed:async()=>({events:[],nextCursor:0,hasMore:false}),event:async()=>{throw Error("none");},webUrl:async()=>null,chat};
const base:UnifiedWorkbenchProps={items:[{id:"bridge-a",name:"战斗 Bridge",lifecycle:"ACTIVE"}],draft:{value:""},onDraftChange:()=>{},onSelectWorkstream:()=>{},watchApi};

it("keeps the conversation list beside the open conversation on wide desktops",async()=>{
 render(<UnifiedWorkbench {...base} surface="BRIDGES"/>);
 const pane=document.querySelector(".v7-list-pane") as HTMLElement;
 expect(pane).toBeVisible();expect(within(pane).getByRole("heading",{name:"对话"})).toBeVisible();
 expect(screen.getByText("选择左侧对话查看详情")).toBeVisible();
 expect(within(pane).getAllByRole("button").map(b=>b.textContent?.trim()).filter(t=>["全部","等你","执行中1","Pi","Bridge"].includes(t??""))).toEqual(["全部","等你","执行中1","Pi","Bridge"]);
 fireEvent.click(within(pane).getByRole("button",{name:/界面重设计/}));
 const header=await waitFor(()=>{const h=document.querySelector(".v4-chat-header") as HTMLElement;expect(h).toBeTruthy();return h;});
 expect(pane).toBeVisible();
 expect(within(pane).getByRole("button",{name:/界面重设计/})).toHaveAttribute("aria-current","true");
 await waitFor(()=>expect(within(header).getByText("Opus 5.5 · high")).toBeVisible());
 expect(within(header).getByRole("status")).toHaveTextContent("正在执行");
 expect(screen.queryByText("选择左侧对话查看详情")).toBeNull();
});

it("keeps the single-column flow on phones",()=>{
 viewport(q=>q.includes("max-width:760px")||q==="(min-width:1024px)");
 render(<UnifiedWorkbench {...base} surface="BRIDGES"/>);
 expect(document.querySelector(".v7-list-pane")).toBeNull();
 expect(document.querySelector(".r2-workspace>.r2-header")).toBeTruthy();
});

it("shows model unknown on rows whose runtime is not projected yet",()=>{
 render(<UnifiedWorkbench {...base} surface="BRIDGES"/>);
 const row=screen.getByRole("button",{name:/界面重设计/});
 expect(within(row).getByText("模型未知")).toBeVisible();
 expect(row.querySelector(".v7-meta-text")).toHaveTextContent("Pi");
});
