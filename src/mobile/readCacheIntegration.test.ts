import {cachedWatchChatApi} from '../features/workbench/watchChatApi';
import {afterEach,expect,it,vi} from "vitest";
import {readModels} from "../features/workbench/readModelCache";
import {mobileRoleBridgeApi} from "./roleBridgeApi";
afterEach(()=>{readModels.clear();readModels.dispose();vi.unstubAllGlobals();vi.useRealTimers();});
it("passive POST synchronization publishes newer Bridge content rather than invalidating its own pending read",async()=>{
 vi.useFakeTimers();readModels.clear();const api=mobileRoleBridgeApi("exact-work");let version=1;
 const fetch=vi.fn(async(_input:RequestInfo|URL,_init?:RequestInit)=>new Response(JSON.stringify({bindings:{workstreamId:"exact-work",bindingRevision:1,explicitRoles:true},replies:[{id:"observation",endpointId:"endpoint",text:`body${version}`}],handoffs:[],snapshotAt:Date.now()})));
 vi.stubGlobal("fetch",fetch);const seen=vi.fn();const stop=api.subscribeState!("exact-work",seen);
 expect((await api.state("exact-work")).replies[0].text).toBe("body1");version=2;await api.sync!("exact-work");await vi.advanceTimersByTimeAsync(2000);
 expect(api.cachedState!("exact-work")?.replies[0].text).toBe("body2");expect(seen).toHaveBeenCalledTimes(2);expect(fetch.mock.calls.at(-1)?.[1]).toMatchObject({method:"POST",body:JSON.stringify({action:"SYNC"})});stop();
});

it('conversation navigation stays warm while explicit status and history retry fetch fresh reads',async()=>{
 readModels.clear();let version=1;const state=vi.fn(async()=>({watch:{threadId:'exact',cwd:'D:/fixture',generation:1,snapshot:{turnId:'t',itemId:'m',state:'RUNNING'}},historyRevision:String(version)}) as any),command=vi.fn(async()=>({messages:[{id:String(version),turnId:'t',role:'assistant',text:'fixture'}],nextCursor:null}) as any);
 const api=cachedWatchChatApi({state,command});await api.state('exact');await api.command({action:'HISTORY',threadId:'exact'});version=2;await api.state('exact');expect(state).toHaveBeenCalledTimes(1);
 await api.refreshState!('exact');expect(state).toHaveBeenCalledTimes(2);const page=await api.refreshHistory!('exact');expect(page.messages[0].id).toBe('2');expect(command).toHaveBeenCalledTimes(2);expect(command.mock.calls.every(c=>(c as any)[0].action==='HISTORY')).toBe(true);
});
